import logging
import uuid
from collections.abc import Awaitable, Callable
from typing import Any

from langchain.agents.middleware import AgentMiddleware, ModelRequest, ModelResponse
from langchain_core.messages import AIMessage, BaseMessage, HumanMessage

from semantic_agent.agent.context import append_system
from semantic_agent.agent.prompts import EMPTY_ANSWER_NOTE
from semantic_agent.api.thought import ThoughtFilter
from semantic_agent.tools.client import REQUEST_ID, SEARCH_FAIL, THREAD_ID, ToolClient

log = logging.getLogger(__name__)

MODEL = "model"


def _text(message: BaseMessage) -> str:
    content = message.content
    if isinstance(content, str):
        return content
    return "".join(b.get("text", "") for b in content if isinstance(b, dict))


def last_question(messages: list[BaseMessage]) -> str | None:
    for message in reversed(messages):
        if isinstance(message, HumanMessage):
            return _text(message)
    return None


def visible(message: AIMessage) -> str:
    thought = ThoughtFilter()
    return (thought.push(_text(message)) + thought.flush()).strip()


class FirstStep(AgentMiddleware):
    def __init__(self, client: ToolClient) -> None:
        super().__init__()
        self._client = client

    async def _step(self, question: str, answer: str | None = None) -> dict[str, Any] | None:
        thread_id = THREAD_ID.get()
        if not thread_id:
            return None
        body: dict[str, Any] = {"request_id": REQUEST_ID.get(), "question": question}
        if answer is not None:
            body["answer"] = answer
        data, err = await self._client.fetch(
            "POST", f"/sessions/{thread_id}/next-step", kind="search", fail=SEARCH_FAIL, json=body
        )
        return data if isinstance(data, dict) and not err else None

    @staticmethod
    def _run(step: dict[str, Any] | None, when: str) -> ModelResponse | None:
        tool = step.get("tool") if step else MODEL
        if step is None or tool == MODEL:
            return None
        log.info("first_step %s tool=%s", when, tool)
        call = {"name": tool, "args": step.get("args") or {}, "id": f"step-{uuid.uuid4().hex}"}
        return ModelResponse(result=[AIMessage(content="", tool_calls=[call])])

    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        question = last_question(list(request.messages))
        forced = self._run(await self._step(question) if question else None, "before")
        if forced is not None:
            return forced
        response = await handler(request)
        final = response.result[-1] if response.result else None
        if not isinstance(final, AIMessage) or final.tool_calls:
            return response
        if not visible(final):
            log.info("first_step empty_answer retry")
            return await handler(request.override(system_message=append_system(request.system_message, EMPTY_ANSWER_NOTE)))
        reread = self._run(await self._step(question, visible(final)) if question else None, "after")
        return reread if reread is not None else response
