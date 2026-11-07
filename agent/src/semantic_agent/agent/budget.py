import logging
from collections.abc import Awaitable, Callable
from typing import Any

from langchain.agents.middleware import AgentMiddleware, ModelRequest, ModelResponse
from langchain_core.messages import AIMessage

from semantic_agent.agent.turn import Turn, current_turn

log = logging.getLogger(__name__)

QUESTION_TOKENS = 300_000
QUESTION_CALLS = 40
LIMIT_TOKENS = "토큰 예산"
LIMIT_CALLS = "모델 호출 수"


class BudgetExceeded(Exception):
    def __init__(self, device: str) -> None:
        super().__init__(device)
        self.device = device


def spent_tokens(response: Any) -> int:
    messages = list(response.result) if isinstance(response, ModelResponse) else [response]
    total = 0
    for message in messages:
        usage = getattr(message, "usage_metadata", None) if isinstance(message, AIMessage) else None
        if usage:
            total += int(usage.get("total_tokens") or 0)
    return total


def over_budget(turn: Turn, tokens: int, calls: int) -> str | None:
    if turn.calls >= calls:
        return LIMIT_CALLS
    if turn.tokens >= tokens:
        return LIMIT_TOKENS
    return None


class QuestionBudget(AgentMiddleware):
    def __init__(self, tokens: int = QUESTION_TOKENS, calls: int = QUESTION_CALLS) -> None:
        super().__init__()
        self._tokens = tokens
        self._calls = calls

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
        response = await handler(request)
        turn.tokens += spent_tokens(response)
        return response
