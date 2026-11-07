from typing import Annotated, Any, Literal

from langchain.tools import ToolRuntime
from langchain_core.tools import BaseTool, tool
from pydantic import Field

from semantic_agent.tools.client import SEARCH_FAIL, Limit, ToolClient, material

Depth = Annotated[int, Field(ge=1, le=2)]


def session_id(runtime: ToolRuntime) -> str:
    return str(runtime.config["configurable"]["thread_id"])


def _join(item: Any) -> str:
    if isinstance(item, dict):
        return " · ".join(str(value) for value in item.values() if value not in (None, ""))
    return str(item)


def format_kg_catalog(data: Any) -> str:
    items = data.get("graphs", []) if isinstance(data, dict) else data or []
    return "\n".join(f"- {_join(item)}" for item in items) or "쓸 수 있는 지식그래프 없음"


def format_nodes(data: dict[str, Any]) -> str:
    lines = []
    for node in data.get("nodes", []):
        lines.append(
            f"{node.get('node', '')} · {node.get('name', '')} · {node.get('kind', '')}"
            f" · {node.get('gloss', '')} · 상태 {node.get('status', '모름')}"
        )
    return "\n".join(lines) or "찾은 노드 없음"


def format_edges(data: dict[str, Any]) -> str:
    lines = []
    for edge in data.get("edges", []):
        neighbor = edge.get("neighbor") or {}
        evidence = edge.get("evidence") or "근거 없음"
        lines.append(
            f"{edge.get('from', '')} -[{edge.get('relation', '')} · 확실성 {edge.get('certainty', '모름')}]-> "
            f"{neighbor.get('node', '')} · {neighbor.get('name', '')}\n"
            f"  근거: {evidence}\n  내용: {neighbor.get('content', '')}"
        )
    return "\n".join(lines) or "펼칠 간선 없음"


def make_kg_tools(client: ToolClient) -> list[BaseTool]:
    @tool("kg_find", description="지식그래프에서 노드를 찾는다. text, 또는 law와 unit으로 찾는다. 짧은 번호(L1…)를 돌려준다.")
    async def kg_find(
        graph: str,
        runtime: ToolRuntime,
        text: str | None = None,
        law: str | None = None,
        unit: str | None = None,
        kind: str | None = None,
        limit: Limit = 20,
    ) -> str:
        if not text and not (law and unit):
            return "인자 오류: text 또는 law와 unit이 필요합니다"
        body = {"graph": graph, "text": text, "law": law, "unit": unit, "kind": kind, "limit": limit}
        path = f"/sessions/{session_id(runtime)}/kg/find"
        data, err = await client.fetch("POST", path, kind="search", fail=SEARCH_FAIL, json=body)
        return err or material(f"지식그래프 {graph}", format_nodes(data))

    @tool("kg_expand", description="지식그래프 노드(짧은 번호)의 간선을 한 방향으로 펼친다. 깊이는 2까지.")
    async def kg_expand(
        graph: str,
        node: str,
        direction: Literal["out", "in"],
        runtime: ToolRuntime,
        relation: str | None = None,
        depth: Depth = 1,
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
        path = f"/sessions/{session_id(runtime)}/kg/expand"
        data, err = await client.fetch("POST", path, kind="search", fail=SEARCH_FAIL, json=body)
        return err or material(f"지식그래프 {graph}", format_edges(data))

    return [kg_find, kg_expand]
