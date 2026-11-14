import logging
from collections.abc import Awaitable, Callable

from langchain.agents.middleware import AgentMiddleware, ModelRequest, ModelResponse
from langchain_core.messages import AIMessage, BaseMessage, HumanMessage, ToolMessage

from semantic_agent.agent.budget import usage_of
from semantic_agent.agent.context import append_system
from semantic_agent.funcall.readcheck import ReadCheck
from semantic_agent.tools.client import ToolClient
from semantic_agent.turn import current_turn

log = logging.getLogger(__name__)

EXCERPT_HEAD = "올린 문서에서 법령이 적힌 줄:"
SEARCH_TOOLS = frozenset(
    {"search_laws", "search_laws_scoped", "search_cases", "read_law", "read_case", "case_citations", "list_annexes", "annex_lookup", "annex_compute"}
)


def excerpt_of(description: str) -> list[str]:
    _, head, rest = description.partition(EXCERPT_HEAD)
    if not head:
        return []
    lines = []
    for line in rest.lstrip("\n").splitlines():
        if not line.strip():
            break
        lines.append(line.strip())
    return lines


def _text(message: BaseMessage) -> str:
    content = message.content
    if isinstance(content, str):
        return content
    return "".join(b.get("text", "") for b in content if isinstance(b, dict))


def searched_in(messages: list[BaseMessage]) -> bool:
    return any(isinstance(m, ToolMessage) and m.name in SEARCH_TOOLS for m in messages)


def task_lines(messages: list[BaseMessage]) -> list[str]:
    first = next((m for m in messages if isinstance(m, HumanMessage)), None)
    return excerpt_of(_text(first)) if first is not None else []


class ResearchGate(AgentMiddleware):
    def __init__(self, client: ToolClient | None, limit_mark: str) -> None:
        super().__init__()
        self._check = ReadCheck(client)
        self._limit_mark = limit_mark

    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        response = await handler(request)
        final = response.result[-1] if response.result else None
        if not isinstance(final, AIMessage) or final.tool_calls or self._limit_mark in _text(final):
            return response
        messages = list((request.state or {}).get("messages", []))
        note = await self._check.review(searched_in(messages), task_lines(messages))
        if not note:
            return response
        current_turn().tokens += usage_of(response)["total"]
        log.info("research review sends the researcher back head=%s", note[:12])
        return await handler(request.override(system_message=append_system(request.system_message, note)))
