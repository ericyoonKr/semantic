from collections.abc import Awaitable, Callable
from typing import Any, NotRequired

from langchain.agents.middleware import AgentMiddleware, AgentState, ModelRequest, ModelResponse
from langchain_core.messages import SystemMessage
from langgraph.config import get_config
from langgraph.store.base import BaseStore

from semantic_agent.memory.store import PREFS_FILE, prefs_namespace

SESSION_MEMO = "/memory/session.md"
OUTLINE_MAX_LEVEL = 2
OUTLINE_MAX_LINES = 60


class UploadsState(AgentState):
    uploads: NotRequired[list[dict[str, Any]]]


def _file_text(state: Any, path: str) -> str:
    entry = (state.get("files") or {}).get(path)
    if not entry:
        return ""
    content = entry.get("content", "") if isinstance(entry, dict) else entry
    return "\n".join(content) if isinstance(content, list) else str(content)


def _upload_lines(uploads: list[dict[str, Any]]) -> str:
    if not uploads:
        return "없음"
    lines = []
    for doc in uploads:
        warnings = ", ".join(str(w) for w in doc.get("warnings") or []) or "없음"
        revision = f" · {doc['revision_of']}의 수정판" if doc.get("revision_of") else ""
        lines.append(
            f"- {doc.get('filename', '')} · {doc.get('format', '')} · {doc.get('pages', '')}쪽"
            f" · 올린 시각 {doc.get('uploaded_at', '')} · 추출 불가: {warnings}{revision} · {doc.get('path', '')}"
        )
        outline = [o for o in doc.get("outline") or [] if int(o.get("level", 1)) <= OUTLINE_MAX_LEVEL]
        for entry in outline[:OUTLINE_MAX_LINES]:
            indent = "  " * int(entry.get("level", 1))
            lines.append(f"{indent}{entry.get('title', '')} ({entry.get('page', '')}쪽)")
    return "\n".join(lines)


async def _prefs_text(store: BaseStore | None) -> str:
    if store is None:
        return ""
    user_id = get_config().get("configurable", {}).get("user_id")
    if not user_id:
        return ""
    item = await store.aget(prefs_namespace(str(user_id)), PREFS_FILE)
    if item is None:
        return ""
    content = item.value.get("content", "")
    return "\n".join(content) if isinstance(content, list) else str(content)


def _append(system: SystemMessage | None, text: str) -> SystemMessage:
    blocks = list(system.content_blocks) if system else []
    blocks.append({"type": "text", "text": f"\n\n{text}" if blocks else text})
    return SystemMessage(content_blocks=blocks)


class ContextMiddleware(AgentMiddleware):
    state_schema = UploadsState

    def __init__(self, judge_catalog: str, kg_catalog: str) -> None:
        super().__init__()
        self._judge_catalog = judge_catalog
        self._kg_catalog = kg_catalog

    async def compose(self, state: Any, store: BaseStore | None) -> str:
        memo = _file_text(state, SESSION_MEMO) or "없음"
        prefs = await _prefs_text(store) or "없음"
        return "\n\n".join(
            [
                f"## 요건 판단 카탈로그\n{self._judge_catalog}",
                f"## 지식그래프 카탈로그\n{self._kg_catalog}",
                f"## 업로드 문서\n{_upload_lines(list(state.get('uploads') or []))}",
                f"## 세션 메모 ({SESSION_MEMO})\n{memo}",
                f"## 사용자 설정 (/prefs{PREFS_FILE})\n{prefs}",
            ]
        )

    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        text = await self.compose(request.state, request.runtime.store)
        return await handler(request.override(system_message=_append(request.system_message, text)))
