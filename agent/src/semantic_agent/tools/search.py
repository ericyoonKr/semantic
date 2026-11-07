from typing import Any

from langchain_core.tools import BaseTool, tool

from semantic_agent.ledger import current_ledger
from semantic_agent.tools.client import SEARCH_FAIL, IsoDate, Limit, ToolClient, material

SNIPPET_CHARS = 300
EXCERPT_NOTE = "발췌라 인용할 수 없음 — 인용하려면 read_law·read_case로 본문을 읽는다"


def ref_line(ref: int) -> str:
    return f"근거 {ref} — 인용할 때 [근거 {ref}]로 쓴다"


def _degraded_line(degraded: Any) -> list[str]:
    if not degraded:
        return []
    if isinstance(degraded, str):
        return [f"강등: {degraded}"]
    return ["강등: 벡터 검색 꺼짐"]


def _version(kind: Any, lsi_seq: Any) -> str:
    return " ".join(str(part) for part in (kind, lsi_seq) if part not in (None, ""))


def format_laws(data: dict[str, Any]) -> str:
    lines = _degraded_line(data.get("degraded"))
    for hit in data.get("hits", []):
        head = (
            f"[{hit.get('law_name', '')} {hit.get('unit_key', '')}({hit.get('title', '')})"
            f" · {hit.get('enforcement_date', '')} · {_version(hit.get('kind'), hit.get('lsi_seq'))}]"
        )
        lines.append(f"{head}\n{str(hit.get('text', ''))[:SNIPPET_CHARS]}")
    if not data.get("hits"):
        lines.append("찾은 조문 없음")
    else:
        lines.append(EXCERPT_NOTE)
    return "\n\n".join(lines)


def format_cases(data: dict[str, Any]) -> str:
    lines = _degraded_line(data.get("degraded"))
    for hit in data.get("hits", []):
        head = (
            f"[{hit.get('court', '')} {hit.get('case_no', '')} · {hit.get('decided_on', '')}"
            f" · {hit.get('field', '')} · {hit.get('section_role', '')} · decision_id {hit.get('decision_id', '')}]"
        )
        lines.append(f"{head}\n{hit.get('text', '')}")
    if not data.get("hits"):
        lines.append("찾은 판례 없음")
    else:
        lines.append(EXCERPT_NOTE)
    return "\n\n".join(lines)


def format_law_unit(data: dict[str, Any]) -> str:
    version = data.get("version") or {}
    head = (
        f"[{data.get('law_name', '')} {data.get('unit_key', '')}({data.get('title', '')})"
        f" · {data.get('enforcement_date', '')}"
        f" · {_version(version.get('kind'), version.get('lsi_seq'))} {version.get('status', '')}]".rstrip()
    )
    if data.get("deleted"):
        head += " 삭제된 단위"
    body = "글자 없음 — 파일 링크만 있음" if data.get("content_status") == "link_only" else str(data.get("body", ""))
    return f"{head}\n{body}"


def format_case(data: dict[str, Any]) -> str:
    head = f"[{data.get('court', '')} {data.get('case_no', '')} · {data.get('decided_on', '')} · {data.get('field', '')}]"
    parts = [head]
    for section in data.get("sections", []):
        parts.append(f"## {section.get('role', '')} — {section.get('heading', '')}\n{section.get('body', '')}")
    return "\n\n".join(parts)


def format_citations(data: dict[str, Any]) -> str:
    lines = []
    for item in data.get("items", []):
        if isinstance(item, dict):
            lines.append("- " + " · ".join(str(value) for value in item.values()))
        else:
            lines.append(f"- {item}")
    if not lines:
        lines.append("인용 관계 없음")
    if data.get("extract_run"):
        lines.append(f"추출 판: {data['extract_run']}")
    return "\n".join(lines)


def make_search_tools(client: ToolClient) -> list[BaseTool]:
    @tool("search_laws", description="법령 조문·별표를 검색한다. 항목마다 법령명·조문·시행일·판본이 붙는다.")
    async def search_laws(
        query: str,
        k: Limit = 8,
        as_of: IsoDate | None = None,
        law_type: list[str] | None = None,
    ) -> str:
        current_ledger().record_query(query)
        body = {"query": query, "k": k, "as_of": as_of, "law_type": law_type}
        data, err = await client.fetch("POST", "/search/laws", kind="search", fail=SEARCH_FAIL, json=body)
        return err or material("법령 검색", format_laws(data))

    @tool("search_cases", description="판례를 검색한다. 항목마다 법원·사건번호·선고일·분야·절이 붙는다.")
    async def search_cases(
        query: str,
        k: Limit = 8,
        field: list[str] | None = None,
        court_level: list[str] | None = None,
        decided_from: IsoDate | None = None,
        decided_to: IsoDate | None = None,
    ) -> str:
        current_ledger().record_query(query)
        body = {
            "query": query,
            "k": k,
            "field": field,
            "court_level": court_level,
            "decided_from": decided_from,
            "decided_to": decided_to,
        }
        data, err = await client.fetch("POST", "/search/cases", kind="search", fail=SEARCH_FAIL, json=body)
        return err or material("판례 검색", format_cases(data))

    @tool("read_law", description="조문·별표 전문을 기준일 판본으로 읽는다.")
    async def read_law(law: str, unit: str, as_of: IsoDate | None = None) -> str:
        params = {"law": law, "unit": unit, "as_of": as_of}
        data, err = await client.fetch("GET", "/laws/unit", kind="search", fail=SEARCH_FAIL, params=params)
        if err:
            return err
        text = format_law_unit(data)
        if data.get("content_status") != "link_only" and not data.get("deleted"):
            text = f"{ref_line(current_ledger().add_law(data).ref)}\n{text}"
        return material(f"{law} {unit}", text)

    @tool("read_case", description="판례를 절 단위로 읽는다. sections를 주면 그 절만 읽는다.")
    async def read_case(decision_id: str, sections: list[str] | None = None) -> str:
        params = {"sections": ",".join(sections) if sections else None}
        data, err = await client.fetch(
            "GET", f"/cases/{decision_id}", kind="search", fail=SEARCH_FAIL, params=params
        )
        if err:
            return err
        text = f"{ref_line(current_ledger().add_case(data).ref)}\n{format_case(data)}"
        return material(f"판례 {decision_id}", text)

    @tool("case_citations", description="판례가 인용한 조문, 또는 조문(law+unit)을 인용한 판례를 찾는다.")
    async def case_citations(
        decision_id: str | None = None,
        law: str | None = None,
        unit: str | None = None,
    ) -> str:
        if not decision_id and not (law and unit):
            return "인자 오류: decision_id 또는 law와 unit이 필요합니다"
        params = {"decision_id": decision_id, "law": law, "unit": unit}
        data, err = await client.fetch("GET", "/citations", kind="search", fail=SEARCH_FAIL, params=params)
        return err or material("인용 관계", format_citations(data))

    return [search_laws, search_cases, read_law, read_case, case_citations]
