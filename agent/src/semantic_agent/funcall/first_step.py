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
from semantic_agent.tools.judge import UPLOAD_KIND
from semantic_agent.turn import current_turn

log = logging.getLogger(__name__)

MODEL = "model"
HISTORY_MAX = 5
LAST_ANSWER_CHARS = 1500
NOTES_HEAD = "이번 차례 메모:"


def with_notes(final: AIMessage, notes: list[str]) -> AIMessage:
    content = final.content if isinstance(final.content, str) else _text(final)
    lines = [n for n in notes if n and n not in content]
    if not lines:
        return final
    return final.model_copy(update={"content": "\n".join(lines) + "\n\n" + content})


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


def recent_turns(messages: list[BaseMessage]) -> tuple[list[str], str | None]:
    questions: list[str] = []
    last_answer: str | None = None
    current_seen = False
    for message in reversed(messages):
        if isinstance(message, HumanMessage):
            if message.additional_kwargs.get("kind") == UPLOAD_KIND:
                continue
            if not current_seen:
                current_seen = True
                continue
            questions.append(_text(message))
            if len(questions) >= HISTORY_MAX:
                break
        elif isinstance(message, AIMessage) and current_seen and last_answer is None and not message.tool_calls:
            shown = visible(message)
            if shown:
                last_answer = shown[:LAST_ANSWER_CHARS]
    questions.reverse()
    return questions, last_answer


class FirstStep(AgentMiddleware):
    def __init__(self, client: ToolClient) -> None:
        super().__init__()
        self._client = client

    async def _step(
        self, question: str, context: tuple[list[str], str | None], answer: str | None = None
    ) -> dict[str, Any] | None:
        thread_id = THREAD_ID.get()
        if not thread_id:
            return None
        history, last_answer = context
        body: dict[str, Any] = {"request_id": REQUEST_ID.get(), "question": question, "history": history}
        if last_answer is not None:
            body["last_answer"] = last_answer
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
        messages = list(request.messages)
        question = last_question(messages)
        context = recent_turns(messages)
        step = await self._step(question, context) if question else None
        turn = current_turn()
        notes = [str(n) for n in ((step or {}).get("notes") or []) if n]
        if notes:
            turn.notes = notes
        forced = self._run(step, "before")
        if forced is not None:
            return forced
        if turn.notes:
            request = request.override(system_message=append_system(request.system_message, f"{NOTES_HEAD} " + " ".join(turn.notes)))
        response = await handler(request)
        final = response.result[-1] if response.result else None
        if not isinstance(final, AIMessage) or final.tool_calls:
            return response
        if not visible(final):
            log.info("first_step empty_answer retry")
            response = await handler(request.override(system_message=append_system(request.system_message, EMPTY_ANSWER_NOTE)))
            final = response.result[-1] if response.result else None
            if not isinstance(final, AIMessage) or final.tool_calls:
                return response
        reread = self._run(await self._step(question, context, visible(final)) if question else None, "after")
        if reread is not None:
            return reread
        if not turn.notes:
            return response
        return ModelResponse(result=[*response.result[:-1], with_notes(final, turn.notes)], structured_response=response.structured_response)
