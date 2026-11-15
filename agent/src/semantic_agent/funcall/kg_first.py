import logging
import uuid
from collections.abc import Awaitable, Callable

from langchain.agents.middleware import AgentMiddleware, ModelRequest, ModelResponse
from langchain_core.messages import AIMessage

from semantic_agent.turn import current_turn

log = logging.getLogger(__name__)


class KgFirst(AgentMiddleware):
    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        turn = current_turn()
        first = turn.researcher_first
        messages = (request.state or {}).get("messages", [])
        if not first or any(isinstance(m, AIMessage) for m in messages):
            return await handler(request)
        turn.researcher_first = None
        args = dict(first.get("args") or {})
        log.info("kg_first tool=%s graph=%s", first.get("tool"), args.get("graph"))
        call = {"name": str(first.get("tool")), "args": args, "id": f"kgfirst-{uuid.uuid4().hex}"}
        return ModelResponse(result=[AIMessage(content="", tool_calls=[call])])
