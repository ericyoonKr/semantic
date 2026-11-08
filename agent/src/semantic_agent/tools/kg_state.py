from typing import Any, Literal

from langchain.tools import ToolRuntime
from langchain_core.messages import HumanMessage, ToolMessage
from langchain_core.tools import BaseTool, tool
from langgraph.types import Command

from semantic_agent.agent.progress import PROGRESS_KEY, STATUS_LABELS
from semantic_agent.tools.client import SEARCH_FAIL, ToolClient
from semantic_agent.tools.judge import is_upload_line
from semantic_agent.tools.kg import session_id

TOOL_NAME = "kg_write_state"
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
            return NO_BASIS
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
