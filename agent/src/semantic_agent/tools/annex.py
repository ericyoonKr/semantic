from typing import Annotated, Any

from langchain_core.tools import BaseTool, tool
from pydantic import Field

from semantic_agent.tools.client import SEARCH_FAIL, IsoDate, ToolClient, material
from semantic_agent.ledger import compact, current_ledger
from semantic_agent.tools.search import failed, ref_line

ANNEX_LIST_KEY = "별표 목록"
RowLimit = Annotated[int, Field(ge=1, le=200)]
Elapsed = Annotated[int, Field(ge=0)]
AMOUNT_RULE = "금액은 이 결과의 원 단위 값만 옮긴다. 기억이나 검색 발췌의 숫자를 쓰지 않는다."


def _text(value: Any) -> str:
    return "" if value is None else str(value)


def _won(value: Any) -> str:
    return f"{value:,}원" if isinstance(value, int) else "값 없음"


def _annex_head(annex: dict[str, Any]) -> str:
    period = f"{_text(annex.get('valid_from'))}~{_text(annex.get('valid_to'))}"
    related = _text(annex.get("title_related"))
    return (
        f"[{_text(annex.get('law_name'))} [{_text(annex.get('annex_label'))}] {_text(annex.get('title'))}"
        f" · 판본 {_text(annex.get('version_id'))} · 효력 {period} · 검수 {_text(annex.get('review_status'))}]"
        + (f" 관련 {related}" if related and related not in _text(annex.get("title")) else "")
    )


def _ordered_cells(row: dict[str, Any], columns: list[list[str]]) -> list[tuple[str, str]]:
    cells = row.get("cells") or {}
    if not isinstance(cells, dict):
        return []
    table = row.get("table")
    order = columns[table] if isinstance(table, int) and 0 <= table < len(columns) else []
    keys = sorted(cells, key=lambda k: order.index(k) if k in order else len(order))
    out = []
    for key in keys:
        cell = cells[key]
        text = cell.get("text") if isinstance(cell, dict) else cell
        if text:
            out.append((key, _text(text)))
    return out


def _amount(a: dict[str, Any]) -> str:
    kind = a.get("kind")
    if kind == "fixed":
        return _won(a.get("krw"))
    if kind == "list":
        return " / ".join(_amount(item) for item in a.get("items") or [] if isinstance(item, dict))
    if kind == "formula":
        return f"산식: {_text(a.get('source'))}"
    return f"금액으로 읽지 못함: {_text(a.get('source'))}"


def format_table(data: dict[str, Any]) -> str:
    annex = data.get("annex") or {}
    columns = data.get("columns") or []
    lines = [_annex_head(annex)]
    lines.extend(f"주의: {note}" for note in data.get("notes") or [])
    if data.get("failed_checks"):
        lines.append("표 복원 검사 실패: " + ", ".join(_text(c) for c in data["failed_checks"]))
    if annex.get("source_urls"):
        lines.append("원본: " + " ".join(_text(u) for u in annex["source_urls"]))
    rows = data.get("rows") or []
    lines.append(f"맞는 행 {data.get('rows_matched', len(rows))}개")
    for row in rows:
        parts = [f"{key}: {text}" for key, text in _ordered_cells(row, columns)]
        head = f"- 행 {_text(row.get('label')) or _text(row.get('path'))} (node_id {_text(row.get('node_id'))})"
        lines.append(head + "\n  " + "\n  ".join(parts or [_text(row.get("text"))]))
        amounts = row.get("amounts") or {}
        for key, a in amounts.items() if isinstance(amounts, dict) else []:
            if isinstance(a, dict):
                lines.append(f"  금액 {key}: {_amount(a)}")
        if row.get("basis_unit_ids"):
            lines.append("  근거 조문 키: " + ", ".join(_text(b) for b in row["basis_unit_ids"]))
    paras = data.get("paras") or []
    if paras:
        lines.append("일반기준·비고(함께 제시):")
        lines.extend(f"- {_text(p.get('label'))} {_text(p.get('text'))}".replace("-  ", "- ") for p in paras)
    related = data.get("related") or []
    if related:
        lines.append(
            "관련 조문: "
            + ", ".join(f"{_text(r.get('law_name'))} {_text(r.get('article_no'))}({_text(r.get('ref_type'))})" for r in related)
        )
    if data.get("raw_text"):
        lines.append("원문:\n" + _text(data["raw_text"]))
    lines.append(AMOUNT_RULE)
    return "\n".join(lines)


def format_computation(data: dict[str, Any]) -> str:
    annex = data.get("annex") or {}
    lines = [_annex_head(annex), f"행 {_text(data.get('label')) or _text(data.get('path'))}: {_text(data.get('row_text'))}"]
    lines.extend(f"주의: {note}" for note in data.get("notes") or [])
    for r in data.get("results") or []:
        item = f" 값{r['item']}" if r.get("item") else ""
        line = f"- {_text(r.get('column'))}{item}: {_won(r.get('krw'))} (칸 원문 「{_text(r.get('cell_text'))}」)"
        if r.get("work"):
            line += f" 계산 {r['work']}"
        if r.get("source") and r.get("kind") == "formula":
            line += f" 산식 원문 「{r['source']}」"
        if r.get("note"):
            line += f" — {r['note']}"
        lines.append(line)
    if not data.get("results"):
        lines.append("계산한 금액 없음")
    paras = data.get("paras") or []
    if paras:
        lines.append("일반기준·비고(적용하지 않음, 함께 제시):")
        lines.extend(f"- {_text(p.get('label'))} {_text(p.get('text'))}".replace("-  ", "- ") for p in paras)
    lines.append(AMOUNT_RULE)
    return "\n".join(lines)


def format_annex_list(data: dict[str, Any]) -> str:
    rows = data.get("annexes") or []
    head = f"[{_text(data.get('law_name'))} 별표·서식 목록 {len(rows)}건]"
    lines = [head]
    lines.extend(f"- {_text(a.get('unit_key'))} {_text(a.get('title'))} (시행 {_text(a.get('enforcement_date'))})" for a in rows)
    if not rows:
        lines.append("별표·서식 없음")
    lines.append("목록의 제목만 쓸 수 있다. 내용을 말하려면 read_law(law, \"별표 N\")로 읽는다.")
    return "\n".join(lines)


def trust_annex_list(data: dict[str, Any]) -> None:
    ledger = current_ledger()
    law = compact(_text(data.get("law_name")))
    for a in data.get("annexes") or []:
        ledger.trust(law + compact(_text(a.get("unit_key"))) + compact(_text(a.get("title"))))


def make_annex_tools(client: ToolClient) -> list[BaseTool]:
    @tool(
        "list_annexes",
        description="법령 하나의 별표·서식 전체 목록(번호·제목)을 기준일 판본으로 가져온다. 별표가 무엇이 있는지 물으면 이것으로 모두 가져온다.",
    )
    async def list_annexes(law: str, as_of: IsoDate | None = None) -> str:
        data, err = await client.fetch("GET", "/laws/annexes", kind="search", fail=SEARCH_FAIL, params={"law": law, "as_of": as_of})
        if err:
            return failed(err)
        trust_annex_list(data)
        text = format_annex_list(data)
        item = current_ledger().add_law(
            {"law_name": data.get("law_name") or law, "unit_key": ANNEX_LIST_KEY, "body": text, "law_id": data.get("law_id"), "version": {"version_id": ANNEX_LIST_KEY}}
        )
        return material(f"{law} 별표 목록", ref_line(item.ref) + "\n" + text)

    @tool(
        "annex_lookup",
        description=(
            "법령 별표(과태료·처분 기준표 등)를 기준일 판본으로 펼쳐 행·열·금액을 읽는다. "
            "row(행 표시), words(행 글에 모두 들어 있어야 할 낱말), column(금액 열 이름 일부)으로 좁힌다. "
            "일반기준·비고 문단과 관련 조문이 함께 온다. 금액을 말하려면 이 결과나 annex_compute 결과만 쓴다."
        ),
    )
    async def annex_lookup(
        law: str,
        annex: str,
        as_of: IsoDate | None = None,
        row: str | None = None,
        words: str | None = None,
        column: str | None = None,
        limit: RowLimit = 50,
    ) -> str:
        body = {
            "law": law,
            "annex": annex,
            "as_of": as_of,
            "row": row,
            "words": words,
            "column": column,
            "limit": limit,
        }
        data, err = await client.fetch("POST", "/annex/lookup", kind="search", fail=SEARCH_FAIL, json=body)
        return failed(err) or material(f"{law} {annex}", format_table(data))

    @tool(
        "annex_compute",
        description=(
            "annex_lookup이 준 node_id 행의 금액을 계산한다. 고정 금액은 그대로, 산식 칸은 elapsed(산식의 일·개월 수)로 계산한다. "
            "column을 주지 않으면 모든 금액 열을 낸다. 일반기준의 가중·감경은 적용하지 않고 문단으로 함께 준다."
        ),
    )
    async def annex_compute(node_id: str, column: str | None = None, elapsed: Elapsed | None = None) -> str:
        body = {"node_id": node_id, "column": column, "elapsed": elapsed}
        data, err = await client.fetch("POST", "/annex/compute", kind="search", fail=SEARCH_FAIL, json=body)
        return failed(err) or material(f"별표 계산 {node_id}", format_computation(data))

    return [list_annexes, annex_lookup, annex_compute]
