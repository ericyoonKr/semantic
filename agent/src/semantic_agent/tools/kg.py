import json
from typing import Annotated, Any, Literal

from langchain.tools import ToolRuntime
from langchain_core.tools import BaseTool, tool
from pydantic import Field

from semantic_agent.tools.client import SEARCH_FAIL, IsoDate, Limit, ToolClient, material

Depth = Annotated[int, Field(ge=1, le=2)]

STATUS_LABELS = {
    "found": "찾음",
    "none_confirmed": "없음",
    "unknown": "모름",
    "out_of_scope": "범위 밖",
    "not_examined": "아직 안 봄",
}
PERIOD_LABELS = {"in_force": "기준일에 효력 있음", "not_checked": "효력 구간이 없어 판단 안 함"}
CHAIN_ONLY = "두 단계 간선은 한 간선씩 읽고, 끝점 사이의 관계로 줄여 읽지 않는다"


def session_id(runtime: ToolRuntime) -> str:
    return str(runtime.config["configurable"]["thread_id"])


def _text(value: Any) -> str:
    if isinstance(value, (dict, list)):
        return json.dumps(value, ensure_ascii=False)
    return str(value)


def _catalog_line(item: dict[str, Any]) -> str:
    parts = [str(item.get("graph", "")), f"모양 {item.get('shape', '')}", f"노드 {item.get('nodes', '')}개"]
    relations = item.get("relations") or []
    if relations:
        parts.append("관계 " + ", ".join(str(r) for r in relations))
    parts.append(f"허용된 두 단계 경로 {item.get('compositions', 0)}개")
    state = item.get("state") or {}
    if state.get("values"):
        parts.append("적을 수 있는 상태 값 " + ", ".join(str(v) for v in state["values"]))
    parts.append(f"효력 구간이 있는 항목 {item.get('dated', 0)}개")
    if not item.get("article_index"):
        parts.append("조문으로 찾기 없음")
    measurement = item.get("measurement")
    if measurement:
        parts.append(f"측정 상태 {_text(measurement)}")
    return " · ".join(parts)


def format_kg_catalog(data: Any) -> str:
    items = data.get("graphs", []) if isinstance(data, dict) else data or []
    lines = [f"- {_catalog_line(item) if isinstance(item, dict) else item}" for item in items]
    disabled = data.get("disabled", []) if isinstance(data, dict) else []
    for item in disabled:
        lines.append(f"- 읽지 못한 묶음: {item.get('bundle', '')} ({item.get('reason', '')})")
    return "\n".join(lines) or "쓸 수 있는 지식그래프 없음"


def catalog_hashes(data: Any) -> dict[str, str]:
    items = data.get("graphs", []) if isinstance(data, dict) else []
    return {str(i.get("graph")): str(i.get("sha256")) for i in items if isinstance(i, dict) and i.get("sha256")}


def _outcome(data: dict[str, Any]) -> list[str]:
    status = data.get("status")
    if status is None:
        return []
    line = f"상태: {STATUS_LABELS.get(status, status)}"
    if data.get("reason"):
        line += f" — {data['reason']}"
    lines = [line]
    for basis in data.get("basis") or []:
        lines.append(f"  찾아본 기록: {basis}")
    as_of = data.get("as_of")
    if isinstance(as_of, dict):
        lines.append(
            f"기준일 {as_of.get('date', '')}: 효력 있음 {as_of.get('in_force', 0)}개, "
            f"효력 구간이 없어 판단 안 함 {as_of.get('not_checked', 0)}개, 기준일 밖이라 뺌 {as_of.get('excluded', 0)}개"
        )
    return lines


def _state(node: dict[str, Any]) -> str:
    state = node.get("status")
    if not isinstance(state, dict):
        return f" · 상태 {state}" if state not in (None, "") else ""
    judgment = state.get("judgment")
    reason = state.get("reason")
    text = " · 상태 " + str(judgment) if judgment not in (None, "") else ""
    return text + (f" ({reason})" if reason else "")


def format_nodes(data: dict[str, Any]) -> str:
    lines = _outcome(data)
    for node in data.get("nodes", []):
        line = f"{node.get('node', '')} · {node.get('name', '')} · {node.get('kind', '')}"
        if node.get("gloss"):
            line += f" · {node['gloss']}"
        lines.append(line + _state(node))
    return "\n".join(lines) or "찾은 노드 없음"


def _evidence(value: Any) -> str:
    if isinstance(value, list):
        return "; ".join(_text(item) for item in value) or "근거 없음"
    return str(value or "근거 없음")


def format_edges(data: dict[str, Any]) -> str:
    lines = _outcome(data)
    edges = data.get("edges", [])
    if data.get("composition_mode") == "chain_only" and any(e.get("depth") == 2 for e in edges):
        lines.append(CHAIN_ONLY)
    if data.get("refused_paths"):
        lines.append(f"계약에 없는 두 단계 경로 {data['refused_paths']}개는 펼치지 않음")
    for edge in edges:
        neighbor = edge.get("neighbor") or {}
        label = str(edge.get("relation", ""))
        if edge.get("certainty") not in (None, ""):
            label += f" · 확실성 {edge['certainty']}"
        lines.append(
            f"{edge.get('from', '')} -[{label}]-> {neighbor.get('node', '')} · {neighbor.get('name', '')}"
            f"\n  근거: {_evidence(edge.get('evidence'))}"
        )
        if edge.get("checks"):
            lines.append("  확인: " + _text(edge["checks"]))
        if edge.get("period") in PERIOD_LABELS:
            lines.append(f"  기준일: {PERIOD_LABELS[edge['period']]}")
        lines.append(f"  내용: {neighbor.get('content', '')}")
    return "\n".join(lines) or "펼칠 간선 없음"


def make_kg_tools(client: ToolClient) -> list[BaseTool]:
    @tool(
        "kg_find",
        description="지식그래프에서 노드를 찾는다. text, 또는 law와 unit으로 찾는다. 짧은 번호(L1…)를 돌려준다. "
        "as_of(YYYY-MM-DD)를 주면 효력 구간이 있는 항목만 기준일로 거른다.",
    )
    async def kg_find(
        graph: str,
        runtime: ToolRuntime,
        text: str | None = None,
        law: str | None = None,
        unit: str | None = None,
        kind: str | None = None,
        as_of: IsoDate | None = None,
        limit: Limit = 20,
    ) -> str:
        if not text and not (law and unit):
            return "인자 오류: text 또는 law와 unit이 필요합니다"
        body = {"graph": graph, "text": text, "law": law, "unit": unit, "kind": kind, "as_of": as_of, "limit": limit}
        path = f"/sessions/{session_id(runtime)}/kg/find"
        data, err = await client.fetch("POST", path, kind="search", fail=SEARCH_FAIL, json=body)
        if err:
            return err
        return material(f"지식그래프 {graph}", format_nodes(data))

    @tool(
        "kg_expand",
        description="지식그래프 노드(짧은 번호)의 간선을 한 방향으로 펼친다. 깊이는 2까지이고, 두 번째 걸음은 그래프 계약이 "
        "허용한 두 단계 경로만 따른다. 결과의 상태(찾음·없음·모름·범위 밖·아직 안 봄)를 바꾸지 않고 옮긴다.",
    )
    async def kg_expand(
        graph: str,
        node: str,
        direction: Literal["out", "in"],
        runtime: ToolRuntime,
        relation: str | None = None,
        depth: Depth = 1,
        as_of: IsoDate | None = None,
        limit: Limit = 20,
    ) -> str:
        body = {
            "graph": graph,
            "node": node,
            "direction": direction,
            "relation": relation,
            "depth": depth,
            "limit": limit,
        }
        if as_of:
            body["as_of"] = as_of
        path = f"/sessions/{session_id(runtime)}/kg/expand"
        data, err = await client.fetch("POST", path, kind="search", fail=SEARCH_FAIL, json=body)
        if err:
            return err
        return material(f"지식그래프 {graph}", format_edges(data))

    return [kg_find, kg_expand]
