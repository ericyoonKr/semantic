import asyncio
from collections.abc import Awaitable, Callable
from typing import Annotated, Any

from langchain_core.tools import BaseTool, tool
from pydantic import Field

from semantic_agent.ledger import current_ledger
from semantic_agent.tools.client import SEARCH_FAIL, IsoDate, Limit, ToolClient, material

SNIPPET_CHARS = 200
MAX_QUERIES = 4
Queries = Annotated[list[Annotated[str, Field(min_length=1)]], Field(min_length=1, max_length=MAX_QUERIES)]
EXCERPT_NOTE = "발췌라 인용할 수 없음 — 인용하려면 read_law·read_case로 본문을 읽는다"
CASE_EXCERPT_NOTE = "발췌라 인용할 수 없음 — 쟁점에 맞는 판례 1~2건을 read_case(decision_id)로 읽어야 근거로 들 수 있다. decision_id는 항목 머리의 값이다"


CASE_DATA_MISSING = "판례 자료가 아직"


def failed(err: str | None) -> str | None:
    if err:
        current_ledger().search_failed = True
    return err


def case_failed(err: str | None) -> str | None:
    if err and CASE_DATA_MISSING in err:
        current_ledger().case_data_missing = err.split(" (요청 ID")[0]
    return failed(err)


def case_data_gone() -> str | None:
    return current_ledger().case_data_missing


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


def _scheduled(value: Any) -> str:
    return f" · 시행 예정: {value}" if value else ""


def _scope_parts(hit: dict[str, Any]) -> tuple[str, str]:
    law_id = f" · 법령 ID {hit['law_id']}" if hit.get("law_id") else ""
    units = hit.get("matched_units") or []
    matched = f"\n맞은 항·호: {', '.join(str(u) for u in units)}" if units else ""
    return law_id, matched


def format_laws(data: dict[str, Any], scoped: bool = False) -> str:
    lines = _degraded_line(data.get("degraded_reason") or data.get("degraded"))
    for hit in data.get("hits", []):
        version = hit.get("version_id") or _version(hit.get("kind"), hit.get("lsi_seq"))
        node = f" · node_id {hit['node_id']}" if hit.get("node_id") else ""
        law_id, matched = _scope_parts(hit) if scoped else ("", "")
        head = (
            f"[{hit.get('law_name', '')} {hit.get('unit_key', '')}({hit.get('title', '')})"
            f" · {hit.get('enforcement_date', '')} · {version}{node}{law_id}{_scheduled(hit.get('scheduled_from'))}]"
        )
        lines.append(f"{head}{matched}\n{str(hit.get('text', ''))[:SNIPPET_CHARS]}")
    if not data.get("hits"):
        lines.append("찾은 조문 없음")
    else:
        lines.append(EXCERPT_NOTE)
    return "\n\n".join(lines)


DISPOSITIONS = {
    "dismiss_appeal": "상고기각",
    "reverse_remand": "파기환송",
    "reverse_render": "파기자판",
    "reverse_transfer": "파기이송",
    "dismiss": "각하",
    "grant": "인용",
    "reject": "기각",
    "partial_grant": "일부인용",
}
OPINIONS = {"majority": "다수의견", "dissent": "반대의견", "separate": "별개의견", "concurring": "보충의견"}
DIRECTIONS = {"positive": "적극", "negative": "소극"}


def case_marks(data: dict[str, Any]) -> str:
    marks = []
    if data.get("en_banc"):
        marks.append("[전원합의체]")
    if data.get("disposition") in DISPOSITIONS:
        marks.append(f"[주문: {DISPOSITIONS[data['disposition']]}]")
    if data.get("overruled_by"):
        marks.append(f"[변경됨: {', '.join(str(c) for c in data['overruled_by'])}]")
    return " ".join(marks)


def section_marks(section: dict[str, Any]) -> str:
    marks = []
    opinion = section.get("opinion")
    if opinion and opinion != "majority":
        marks.append(f"[{OPINIONS.get(opinion, opinion)}]")
    if section.get("direction") in DIRECTIONS:
        marks.append(f"[{DIRECTIONS[section['direction']]}]")
    if section.get("lower_court_view") is True:
        marks.append("[원심 판단 부분]")
    return " ".join(marks)


def _marked(head: str, *marks: str) -> str:
    return " ".join(part for part in (head, *marks) if part)


def format_cases(data: dict[str, Any]) -> str:
    lines = _degraded_line(data.get("degraded_reason") or data.get("degraded"))
    for hit in data.get("hits", []):
        head = (
            f"[{hit.get('court', '')} {hit.get('case_no', '')} · {hit.get('decided_on', '')}"
            f" · {hit.get('field', '')} · {hit.get('section_role', '')} · decision_id {hit.get('decision_id', '')}]"
        )
        lines.append(f"{_marked(head, case_marks(hit), section_marks(hit))}\n{hit.get('text', '')}")
    if not data.get("hits"):
        lines.append("찾은 판례 없음")
    else:
        lines.append(CASE_EXCERPT_NOTE)
    return "\n\n".join(lines)


SHOWN_MAX_CHARS = 12_000


def clip_shown(text: str) -> str:
    if len(text) <= SHOWN_MAX_CHARS:
        return text
    cut = text.rfind("\n", 0, SHOWN_MAX_CHARS)
    cut = cut if cut > SHOWN_MAX_CHARS // 2 else SHOWN_MAX_CHARS
    return f"{text[:cut]}\n(이하 {len(text) - cut:,}자 생략. 인용은 보이는 부분에서만 하고, 뒷부분이 필요하면 범위를 좁혀 다시 읽는다.)"


def format_law_unit(data: dict[str, Any]) -> str:
    version = data.get("version") or {}
    head = (
        f"[{data.get('law_name', '')} {data.get('unit_key', '')}({data.get('title', '')})"
        f" · {data.get('enforcement_date', '')}"
        f" · {version.get('version_id') or _version(version.get('kind'), version.get('lsi_seq'))}"
        f" {version.get('status', '')}{_scheduled(data.get('scheduled_from'))}]".rstrip()
    )
    if data.get("deleted"):
        head += " 삭제된 단위"
    body = "글자 없음 — 파일 링크만 있음" if data.get("content_status") == "link_only" else str(data.get("body", ""))
    return f"{head}\n{body}"


def _lower_view_note(section: dict[str, Any]) -> str:
    spans = section.get("lower_court_view") or []
    if not isinstance(spans, list) or not spans:
        return ""
    ranges = ", ".join(f"{a}–{b}" for a, b in spans)
    return f"[원심 판단 부분: 글자 {ranges}]"


def format_case(data: dict[str, Any]) -> str:
    head = f"[{data.get('court', '')} {data.get('case_no', '')} · {data.get('decided_on', '')} · {data.get('field', '')}]"
    parts = [_marked(head, case_marks(data))]
    for section in data.get("sections", []):
        opinion = OPINIONS.get(section.get("opinion") or "", "")
        direction = DIRECTIONS.get(section.get("direction") or "", "")
        tags = " ".join(f"[{t}]" for t in (opinion, direction) if t)
        title = _marked(f"## {section.get('role', '')} — {section.get('heading', '')}", tags, _lower_view_note(section))
        parts.append(f"{title}\n{section.get('body', '')}")
    return "\n\n".join(parts)


def _cited_line(item: dict[str, Any]) -> str:
    law = item.get("law_name") or item.get("law_id") or "법령 찾지 못함"
    unit = item.get("unit_key") or ""
    version = f" · 판본 {item['article_version_id']}" if item.get("article_version_id") else ""
    key = f" · 조문 키 {item['article_key']}" if item.get("article_key") else ""
    return f"- {law} {unit}{version}{key} · 원문 「{item.get('raw', '')}」"


def _citing_line(item: dict[str, Any]) -> str:
    version = f" · 판본 {item['article_version_id']}" if item.get("article_version_id") else ""
    return (
        f"- {item.get('court', '')} {item.get('case_no', '')} · {item.get('decided_on', '')}"
        f" · decision_id {item.get('decision_id', '')} · 인용 {item.get('unit_key', '')}{version}"
    )


def format_citations(data: dict[str, Any]) -> str:
    lines = []
    for item in data.get("items", []):
        if isinstance(item, dict) and "raw" in item:
            lines.append(_cited_line(item))
        elif isinstance(item, dict) and "case_no" in item:
            lines.append(_citing_line(item))
        elif isinstance(item, dict):
            lines.append("- " + " · ".join(str(value) for value in item.values()))
        else:
            lines.append(f"- {item}")
    if not lines:
        lines.append("인용 관계 없음")
    if data.get("extract_run"):
        lines.append(f"추출 판: {data['extract_run']}")
    return "\n".join(lines)


async def run_queries(queries: list[str], one: Callable[[str], Awaitable[str]]) -> str:
    unique = list(dict.fromkeys(q.strip() for q in queries if q.strip()))
    for q in unique:
        current_ledger().record_query(q)
    results = await asyncio.gather(*(one(q) for q in unique))
    if len(unique) == 1:
        return results[0]
    parts = [f"### 검색어 {i}: {q}\n{r}" for i, (q, r) in enumerate(zip(unique, results, strict=True), start=1)]
    return "\n\n".join(parts)


LAWS_DESCRIPTION = (
    f"법령 조문·별표를 검색한다. 검색어를 {MAX_QUERIES}개까지 한 번에 주면 동시에 찾는다. 항목마다 법령명·조문·시행일·판본이 붙는다."
)
SCOPE_DESCRIPTION = " 항목에 법령 ID와 맞은 항·호가 붙는다. law_ids에 법령 ID를 주면 그 법령 안에서만 찾는다."


def make_search_tools(client: ToolClient, scoped: bool = False) -> list[BaseTool]:
    async def find_laws(
        queries: list[str], k: int, as_of: str | None, law_type: list[str] | None, law_ids: list[str] | None
    ) -> str:
        async def one(query: str) -> str:
            body: dict[str, Any] = {"query": query, "k": k, "as_of": as_of, "law_type": law_type}
            if law_ids:
                body["law_ids"] = law_ids
            data, err = await client.fetch("POST", "/search/laws", kind="search", fail=SEARCH_FAIL, json=body)
            return failed(err) or material("법령 검색", format_laws(data, scoped))

        return await run_queries(queries, one)

    @tool("search_laws", description=LAWS_DESCRIPTION)
    async def search_laws(
        queries: Queries,
        k: Limit = 5,
        as_of: IsoDate | None = None,
        law_type: list[str] | None = None,
    ) -> str:
        return await find_laws(queries, k, as_of, law_type, None)

    @tool("search_laws", description=LAWS_DESCRIPTION + SCOPE_DESCRIPTION)
    async def search_laws_scoped(
        queries: Queries,
        k: Limit = 5,
        as_of: IsoDate | None = None,
        law_type: list[str] | None = None,
        law_ids: list[str] | None = None,
    ) -> str:
        return await find_laws(queries, k, as_of, law_type, law_ids)

    @tool(
        "search_cases",
        description=(
            f"판례를 검색한다. 검색어를 {MAX_QUERIES}개까지 한 번에 주면 동시에 찾는다. 항목마다 법원·사건번호·선고일·분야·절이 붙고,"
            " 전원합의체·주문·적극/소극·변경 여부·원심 판단 부분 표시가 붙는다. 기본은 다수의견 절만 찾는다."
            " include_minority가 참이면 반대·별개·보충의견 절도 찾는다."
            " article_keys에 조문 키(법령 검색 결과 판본 ID의 @ 앞부분)를 주면 그 조문을 인용한 판례도 후보에 넣는다."
        ),
    )
    async def search_cases(
        queries: Queries,
        k: Limit = 5,
        field: list[str] | None = None,
        court_level: list[str] | None = None,
        decided_from: IsoDate | None = None,
        decided_to: IsoDate | None = None,
        include_minority: bool = False,
        article_keys: list[str] | None = None,
    ) -> str:
        async def one(query: str) -> str:
            if gone := case_data_gone():
                return gone
            body = {
                "query": query,
                "k": k,
                "field": field,
                "court_level": court_level,
                "decided_from": decided_from,
                "decided_to": decided_to,
                "include_minority": include_minority,
                "article_keys": article_keys,
            }
            data, err = await client.fetch("POST", "/search/cases", kind="search", fail=SEARCH_FAIL, json=body)
            return case_failed(err) or material("판례 검색", format_cases(data))

        return await run_queries(queries, one)

    @tool("read_law", description="조문·별표 전문을 기준일 판본으로 읽는다.")
    async def read_law(law: str, unit: str, as_of: IsoDate | None = None) -> str:
        params = {"law": law, "unit": unit, "as_of": as_of}
        data, err = await client.fetch("GET", "/laws/unit", kind="search", fail=SEARCH_FAIL, params=params)
        if err:
            return failed(err)
        text = clip_shown(format_law_unit(data))
        if data.get("content_status") != "link_only" and not data.get("deleted"):
            text = f"{ref_line(current_ledger().add_law(data).ref)}\n{text}"
        return material(f"{law} {unit}", text)

    @tool(
        "read_case",
        description=(
            "판례를 절 단위로 읽는다. decision_id는 search_cases·case_citations 결과 항목의 decision_id 값이다."
            " 읽은 결과에 근거 번호가 붙어 인용할 수 있다. sections를 주면 그 절만 읽는다. 기본은 다수의견 절만 읽는다."
            " include_minority가 참이면 반대·별개·보충의견 절도 읽고, 절마다 의견 종류가 붙는다."
        ),
    )
    async def read_case(decision_id: str, sections: list[str] | None = None, include_minority: bool = False) -> str:
        if gone := case_data_gone():
            return gone
        params = {"sections": ",".join(sections) if sections else None, "include_minority": include_minority}
        data, err = await client.fetch(
            "GET", f"/cases/{decision_id}", kind="search", fail=SEARCH_FAIL, params=params
        )
        if err:
            return case_failed(err) or err
        text = f"{ref_line(current_ledger().add_case(data, decision_id).ref)}\n{clip_shown(format_case(data))}"
        return material(f"판례 {decision_id}", text)

    @tool("case_citations", description="판례가 인용한 조문, 또는 조문(law+unit)을 인용한 판례를 찾는다.")
    async def case_citations(
        decision_id: str | None = None,
        law: str | None = None,
        unit: str | None = None,
    ) -> str:
        if not decision_id and not (law and unit):
            return "인자 오류: decision_id 또는 law와 unit이 필요합니다"
        if gone := case_data_gone():
            return gone
        params = {"decision_id": decision_id, "law": law, "unit": unit}
        data, err = await client.fetch("GET", "/citations", kind="search", fail=SEARCH_FAIL, params=params)
        return case_failed(err) or material("인용 관계", format_citations(data))

    return [search_laws_scoped if scoped else search_laws, search_cases, read_law, read_case, case_citations]
