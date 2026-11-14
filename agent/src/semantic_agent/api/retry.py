from typing import Any

from langchain_core.messages import AIMessage, HumanMessage, RemoveMessage
from langgraph.store.base import BaseStore

from semantic_agent.api.answers import delete_answer
from semantic_agent.tools.judge import is_upload_line

SUMMARY_SOURCE = "summarization"


def is_summary(message: Any) -> bool:
    return isinstance(message, HumanMessage) and message.additional_kwargs.get("lc_source") == SUMMARY_SOURCE


def _text(message: HumanMessage) -> str:
    content = message.content
    if isinstance(content, str):
        return content
    return "".join(b.get("text", "") for b in content if isinstance(b, dict))


def retry_cut(messages: list[Any]) -> tuple[str | None, list[Any]]:
    for index in range(len(messages) - 1, -1, -1):
        message = messages[index]
        if isinstance(message, HumanMessage) and not is_upload_line(message) and not is_summary(message):
            dropped = [m for m in messages[index:] if not is_upload_line(m) and not is_summary(m)]
            return _text(message), dropped
    return None, []


async def rewind(agent: Any, store: BaseStore, cfg: dict[str, Any], user_id: str, session_id: str, dropped: list[Any]) -> int:
    removals = [RemoveMessage(id=m.id) for m in dropped if m.id]
    if removals:
        await agent.aupdate_state(cfg, {"messages": removals}, as_node="model")
    for message in dropped:
        if isinstance(message, AIMessage) and not message.tool_calls and message.id:
            await delete_answer(store, user_id, session_id, message.id)
    return len(removals)
