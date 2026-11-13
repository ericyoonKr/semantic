import logging
import uuid
from collections.abc import Awaitable, Callable

from langchain.agents.middleware import AgentMiddleware, ModelRequest, ModelResponse
from langchain_core.messages import AIMessage, BaseMessage, HumanMessage, ToolMessage

from semantic_agent.agent.context import append_system
from semantic_agent.agent.delegation import ISSUE_HEAD, PURPOSE_HEAD, TASK
from semantic_agent.agent.prompts import EMPTY_ANSWER_NOTE
from semantic_agent.agent.subagents import RESEARCHER
from semantic_agent.api.thought import ThoughtFilter

log = logging.getLogger(__name__)

FIRST_PURPOSE = "조회"


def _text(message: BaseMessage) -> str:
    content = message.content
    if isinstance(content, str):
        return content
    return "".join(b.get("text", "") for b in content if isinstance(b, dict))


def _this_turn(messages: list[BaseMessage]) -> tuple[str | None, list[BaseMessage]]:
    for index in range(len(messages) - 1, -1, -1):
        if isinstance(messages[index], HumanMessage):
            return _text(messages[index]), messages[index + 1 :]
    return None, []


def researched(turn: list[BaseMessage]) -> bool:
    return any(isinstance(m, ToolMessage) and m.name == TASK for m in turn) or any(
        isinstance(m, AIMessage) and any(c["name"] == TASK for c in m.tool_calls) for m in turn
    )


def research_call(question: str) -> AIMessage:
    description = f"{ISSUE_HEAD} {' '.join(question.split())}\n{PURPOSE_HEAD} {FIRST_PURPOSE}"
    call = {"name": TASK, "args": {"description": description, "subagent_type": RESEARCHER}, "id": f"first-{uuid.uuid4().hex}"}
    return AIMessage(content="", tool_calls=[call])


def visible(message: AIMessage) -> str:
    thought = ThoughtFilter()
    return (thought.push(_text(message)) + thought.flush()).strip()


def _final(response: ModelResponse) -> AIMessage | None:
    last = response.result[-1] if response.result else None
    return last if isinstance(last, AIMessage) and not last.tool_calls else None


class ResearchFirst(AgentMiddleware):
    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        question, turn = _this_turn(list(request.messages))
        if question is not None and question.strip() and not researched(turn):
            log.info("research_first forced")
            return ModelResponse(result=[research_call(question)])
        response = await handler(request)
        final = _final(response)
        if final is None or visible(final):
            return response
        log.info("research_first empty_answer retry")
        retry = request.override(system_message=append_system(request.system_message, EMPTY_ANSWER_NOTE))
        return await handler(retry)
