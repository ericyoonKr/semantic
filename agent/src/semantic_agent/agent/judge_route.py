import logging
import re
import uuid
from collections.abc import Awaitable, Callable, Iterable
from typing import Any

from langchain.agents.middleware import AgentMiddleware, ModelRequest, ModelResponse
from langchain_core.messages import AIMessage, HumanMessage, ToolMessage

from semantic_agent.turn import current_turn

log = logging.getLogger(__name__)

JUDGE = "judge"
CLAIM_PREFIX = "CLAIM_"
_ASK = re.compile(
    r"(성립|해당하|해당되|해당할|해당합|충족|인정되|인정될|인정돼|지났|도과|청구할 수 있|받을 수 있|이길 수 있"
    r"|책임이 있|책임 있|되는지|갖춰졌|판단해)"
)
_FORM = re.compile(
    r"([?？]|나요?\s*$|니\s*$|까요?\s*$|는지|은가\s*$|는가\s*$|습니까\s*$|봐 ?줘\s*$|알려 ?줘\s*$|판단해 ?줘\s*$"
    r"|아니야\s*$|을까\s*$)"
)


def _compact(text: str) -> str:
    return "".join(text.split())


def route_claim(question: str, claims: dict[str, str]) -> str | None:
    text = question.strip()
    if not (_ASK.search(text) and _FORM.search(text)):
        return None
    flat = _compact(text)
    hits = [claim for claim, word in claims.items() if word and word in flat]
    return hits[0] if len(hits) == 1 else None


def _this_turn(messages: list[Any]) -> tuple[str, list[Any]]:
    for index in range(len(messages) - 1, -1, -1):
        message = messages[index]
        if isinstance(message, HumanMessage):
            return str(message.text), messages[index + 1 :]
    return "", []


class JudgeRoute(AgentMiddleware):
    def __init__(self, claim_types: Iterable[str]) -> None:
        super().__init__()
        self._claims = {c: c.removeprefix(CLAIM_PREFIX) for c in claim_types if c.startswith(CLAIM_PREFIX)}

    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        question, after = _this_turn(list(request.messages))
        if any(isinstance(m, ToolMessage) and m.name == JUDGE for m in after):
            return await handler(request)
        if not any(getattr(t, "name", None) == JUDGE for t in request.tools or []):
            return await handler(request)
        claim = route_claim(question, self._claims)
        if claim is None:
            return await handler(request)
        log.info("judge routed claim=%s", claim)
        current_turn().audit("judge_route", {"claim": claim})
        call = {"name": JUDGE, "args": {"claim_type": claim, "node_id": claim}, "id": f"route-{uuid.uuid4().hex[:12]}"}
        return ModelResponse(result=[AIMessage(content="", tool_calls=[call])])
