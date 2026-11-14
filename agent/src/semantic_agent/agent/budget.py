import logging
from collections.abc import Awaitable, Callable
from typing import Any

from langchain.agents.middleware import AgentMiddleware, ModelRequest, ModelResponse
from langchain_core.messages import AIMessage, HumanMessage, ToolMessage

from semantic_agent.agent.context import append_system
from semantic_agent.agent.prompts import FINISH_NOTE
from semantic_agent.turn import Turn, current_turn

log = logging.getLogger(__name__)

QUESTION_TOKENS = 300_000
QUESTION_CALLS = 40
ERROR_WRAP = 3
LIMIT_TOKENS = "토큰 예산"
LIMIT_CALLS = "모델 호출 수"


def errors_this_turn(messages: list[Any]) -> int:
    count = 0
    for message in reversed(messages):
        if isinstance(message, HumanMessage):
            break
        if isinstance(message, ToolMessage) and message.status == "error":
            count += 1
    return count


class BudgetExceeded(Exception):
    def __init__(self, device: str) -> None:
        super().__init__(device)
        self.device = device


def usage_of(response: Any) -> dict[str, int]:
    messages = list(response.result) if isinstance(response, ModelResponse) else [response]
    out = {"input": 0, "cache_read": 0, "output": 0, "total": 0}
    for message in messages:
        usage = getattr(message, "usage_metadata", None) if isinstance(message, AIMessage) else None
        if not usage:
            continue
        details = usage.get("input_token_details") or {}
        out["input"] += int(usage.get("input_tokens") or 0)
        out["cache_read"] += int(details.get("cache_read") or 0)
        out["output"] += int(usage.get("output_tokens") or 0)
        out["total"] += int(usage.get("total_tokens") or 0)
    return out


def spent_tokens(response: Any) -> int:
    return usage_of(response)["total"]


def over_budget(turn: Turn, tokens: int, calls: int) -> str | None:
    if turn.calls - turn.round_calls >= calls:
        return LIMIT_CALLS
    if turn.tokens - turn.round_tokens >= tokens:
        return LIMIT_TOKENS
    return None


class QuestionBudget(AgentMiddleware):
    def __init__(
        self, name: str = "main", tokens: int = QUESTION_TOKENS, calls: int = QUESTION_CALLS, finish_note: str = FINISH_NOTE
    ) -> None:
        super().__init__()
        self._name = name
        self._tokens = tokens
        self._calls = calls
        self._finish_note = finish_note

    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        turn = current_turn()
        device = over_budget(turn, self._tokens, self._calls)
        if device is not None:
            log.info("budget exceeded device=%s calls=%d tokens=%d", device, turn.calls, turn.tokens)
            raise BudgetExceeded(device)
        turn.calls += 1
        messages = list(getattr(request, "messages", None) or [])
        if not turn.wrap_up and errors_this_turn(messages) >= ERROR_WRAP:
            turn.wrap_up = True
            log.info("tool errors reached %d agent=%s; finishing without tools", ERROR_WRAP, self._name)
        if turn.wrap_up:
            request = request.override(tools=[], system_message=append_system(request.system_message, self._finish_note))
        response = await handler(request)
        used = usage_of(response)
        turn.tokens += used["total"]
        log.info(
            "model_call agent=%s call=%d input=%d cache_read=%d output=%d total=%d cumulative=%d",
            self._name,
            turn.calls,
            used["input"],
            used["cache_read"],
            used["output"],
            used["total"],
            turn.tokens,
        )
        return response
