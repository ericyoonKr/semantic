from typing import Any, Literal

from langchain.tools import ToolRuntime
from langchain_core.messages import HumanMessage, ToolMessage
from langchain_core.tools import BaseTool, tool
from langgraph.types import Command

from semantic_agent.agent.progress import PROGRESS_KEY, STATUS_LABELS, fold
from semantic_agent.tools.client import SEARCH_FAIL, ToolClient
from semantic_agent.tools.judge import is_upload_line
from semantic_agent.tools.kg import session_id

TOOL_NAME = "kg_write_state"
TREE_TOOL = "kg_judge_tree"
NO_BASIS = (
    "적지 않음: 근거를 이 대화의 사용자 진술이나 올린 문서에서 찾지 못했습니다 — 진술 원문이나 문서 경로를 그대로 넣습니다"
)


def _said(messages: list[Any]) -> list[str]:
    return [
        m.content
        for m in messages
        if isinstance(m, HumanMessage) and not is_upload_line(m) and isinstance(m.content, str)
    ]


def basis_found(state: Any, basis: str) -> bool:
    text = basis.strip()
    if not text:
        return False
    uploads = [doc for doc in state.get("uploads") or [] if isinstance(doc, dict)]
    if any(text in (str(doc.get("path", "")), str(doc.get("filename", ""))) for doc in uploads):
        return True
    return any(text in said for said in _said(list(state.get("messages") or [])))


def no_basis(state: Any) -> str:
    names = [
        str(doc.get(key, ""))
        for doc in state.get("uploads") or []
        if isinstance(doc, dict)
        for key in ("filename", "path")
        if doc.get(key)
    ]
    if not names:
        return NO_BASIS
    return NO_BASIS + " — 올린 문서를 근거로 들 때는 basis에 다음 가운데 하나를 그대로 넣습니다: " + ", ".join(names)


def make_kg_state_tool(client: ToolClient) -> BaseTool:
    @tool(
        TOOL_NAME,
        description="이 대화의 사건 진행 상태에 지식그래프 노드(짧은 번호)의 값을 적는다. 값은 카탈로그의 「적을 수 있는 상태 값」 "
        "가운데 하나다. basis에는 사용자 진술 원문이나 올린 문서 경로를 그대로 넣는다. status는 proposed(제안), "
        "confirmed(사용자가 확인함), retracted(앞서 적은 같은 값을 철회)다. 적은 것은 이 대화 안에서만 남는다.",
    )
    async def kg_write_state(
        graph: str,
        node: str,
        value: str,
        basis: str,
        runtime: ToolRuntime,
        status: Literal["proposed", "confirmed", "retracted"] = "proposed",
    ) -> Command | str:
        if not basis_found(runtime.state, basis):
            return no_basis(runtime.state)
        body = {"graph": graph, "node": node, "value": value}
        path = f"/sessions/{session_id(runtime)}/kg/state-check"
        data, err = await client.fetch("POST", path, kind="search", fail=SEARCH_FAIL, json=body)
        if err:
            return err
        if not data.get("accepted"):
            return f"적지 않음: {data.get('reason', '')}"
        row = {
            "graph": graph,
            "node": node,
            "name": data.get("name", ""),
            "value": value.strip(),
            "basis": basis.strip(),
            "status": status,
            "sha256": data.get("sha256", ""),
            "turn": len(_said(list(runtime.state.get("messages") or []))),
        }
        label = STATUS_LABELS.get(status, "철회")
        text = f"적음: {node} {row['name']} = {row['value']} ({label})"
        message = ToolMessage(content=text, tool_call_id=runtime.tool_call_id or "", name=TOOL_NAME)
        return Command(update={PROGRESS_KEY: [row], "messages": [message]})

    return kg_write_state


def written_states(rows: list[dict[str, Any]], graph: str) -> dict[str, str]:
    return {str(r.get("node", "")): str(r.get("value", "")) for r in fold(rows) if r.get("graph") == graph}


def format_tree(data: dict[str, Any]) -> str:
    lines = [f"뿌리 {data.get('root', '')}: {data.get('value', '')}"]
    for n in data.get("nodes") or []:
        blocked = n.get("blocked_by") or []
        tail = f" · 항변 {', '.join(blocked)} 때문에 막힘" if blocked else ""
        lines.append(f"- {n.get('node', '')} {n.get('name', '')}: {n.get('value', '')}{tail}")
    opened = data.get("open_leaves") or []
    if opened:
        lines.append("아직 정하지 않은 잎: " + ", ".join(f"{n.get('node', '')} {n.get('name', '')}" for n in opened))
    ignored = data.get("ignored_states") or []
    if ignored:
        lines.append("쓰지 않은 상태(나무 상태 값이 아니거나 번호를 모름): " + ", ".join(ignored))
    return "\n".join(lines)


def make_kg_tree_tool(client: ToolClient) -> BaseTool:
    @tool(
        TREE_TOOL,
        description="모양이 「나무」인 지식그래프에서 뿌리 노드(짧은 번호)부터 요건(element)·항변(defense)·재항변(rebuttal)을 따라 내려가, "
        "이 대화에 kg_write_state로 적은 잎의 값(충족·불충족·모름·주장됨)을 아래에서 위로 접는다. AND는 모두 충족일 때, OR는 하나라도 "
        "충족일 때 충족이고, 충족인 항변은 그 노드를 막는다. 적지 않은 잎은 모름으로 본다.",
    )
    async def kg_judge_tree(graph: str, root: str, runtime: ToolRuntime) -> str:
        states = written_states(list(runtime.state.get(PROGRESS_KEY) or []), graph)
        body = {"graph": graph, "root": root, "states": states}
        path = f"/sessions/{session_id(runtime)}/kg/tree"
        data, err = await client.fetch("POST", path, kind="search", fail=SEARCH_FAIL, json=body)
        if err:
            return err
        return format_tree(data)

    return kg_judge_tree
