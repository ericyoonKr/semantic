import logging
import re
import uuid
from collections.abc import Awaitable, Callable
from typing import Any

from langchain.agents.middleware import AgentMiddleware, ModelRequest, ModelResponse
from langchain_core.messages import AIMessage, BaseMessage, HumanMessage, ToolMessage

from semantic_agent.agent.context import append_system
from semantic_agent.agent.delegation import EXCERPT_HEAD, ISSUE_HEAD, PURPOSE_HEAD, TASK
from semantic_agent.agent.prompts import EMPTY_ANSWER_NOTE
from semantic_agent.agent.subagents import RESEARCHER
from semantic_agent.api.thought import ThoughtFilter

log = logging.getLogger(__name__)

FIRST_PURPOSE = "조회"
READ_FILE = "read_file"
GREP = "grep"
DOC_TOOLS = frozenset({READ_FILE, GREP})
WHOLE_READ_CHARS = 20_000
WHOLE_READ_LINES = 2_000
EXCERPT_CHARS = 1_500
KEYWORD_MIN = 2
STRIP = "?!.,·「」『』()[]\"'"
PARTICLE = re.compile(r"(?:은|는|이|가|을|를|에|에서|의|로|으로|도|만|과|와|이란|란|인가|인지)$")
LAW_LINE = re.compile(r"「[^」]{2,60}」|[가-힣]+법(?:률)?\s*(?:시행령|시행규칙)?\s*제\s*\d+\s*조")


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


def _called(turn: list[BaseMessage], name: str) -> bool:
    return any(isinstance(m, ToolMessage) and m.name == name for m in turn) or any(
        isinstance(m, AIMessage) and any(c["name"] == name for c in m.tool_calls) for m in turn
    )


def researched(turn: list[BaseMessage]) -> bool:
    return _called(turn, TASK)


def _touched(messages: list[BaseMessage], path: str) -> bool:
    return any(
        isinstance(m, AIMessage) and any(path in str(c.get("args", {})) for c in m.tool_calls if c["name"] in DOC_TOOLS)
        for m in messages
    )


def _chars(state: Any, doc: dict[str, Any]) -> int:
    if isinstance(doc.get("chars"), int):
        return doc["chars"]
    content = ((state.get("files") or {}).get(doc.get("path")) or {}).get("content", "")
    return sum(len(line) + 1 for line in content) if isinstance(content, list) else len(str(content))


def keyword(question: str) -> str | None:
    words = [PARTICLE.sub("", w.strip(STRIP)) for w in question.split()]
    words = [w for w in words if len(w) >= KEYWORD_MIN]
    return max(words, key=len) if words else None


def _uploads(state: Any) -> list[dict[str, Any]]:
    return [d for d in (state.get("uploads") or []) if isinstance(d, dict) and d.get("path")]


def doc_step(state: Any, messages: list[BaseMessage], whole_limit: int, question: str = "") -> AIMessage | None:
    for doc in reversed(_uploads(state)):
        path = str(doc["path"])
        if _touched(messages, path):
            continue
        if _chars(state, doc) <= whole_limit:
            args: dict[str, Any] = {"file_path": path, "limit": WHOLE_READ_LINES}
            return AIMessage(content="", tool_calls=[{"name": READ_FILE, "args": args, "id": f"doc-{uuid.uuid4().hex}"}])
        word = keyword(question)
        if word:
            args = {"pattern": word, "path": path, "output_mode": "content"}
            return AIMessage(content="", tool_calls=[{"name": GREP, "args": args, "id": f"doc-{uuid.uuid4().hex}"}])
    return None


def _lines(state: Any, path: str) -> list[str]:
    content = ((state.get("files") or {}).get(path) or {}).get("content", "")
    return list(content) if isinstance(content, list) else str(content).splitlines()


def law_lines(state: Any) -> str:
    out: list[str] = []
    size = 0
    for doc in reversed(_uploads(state)):
        for line in _lines(state, str(doc["path"])):
            text = " ".join(line.split())
            if not LAW_LINE.search(text) or text in out:
                continue
            if size + len(text) > EXCERPT_CHARS:
                return "\n".join(out)
            out.append(text)
            size += len(text)
    return "\n".join(out)


def research_call(question: str, excerpt: str = "") -> AIMessage:
    description = f"{ISSUE_HEAD} {' '.join(question.split())}\n{PURPOSE_HEAD} {FIRST_PURPOSE}"
    if excerpt:
        description += f"\n{EXCERPT_HEAD}\n{excerpt}"
    call = {"name": TASK, "args": {"description": description, "subagent_type": RESEARCHER}, "id": f"first-{uuid.uuid4().hex}"}
    return AIMessage(content="", tool_calls=[call])


def visible(message: AIMessage) -> str:
    thought = ThoughtFilter()
    return (thought.push(_text(message)) + thought.flush()).strip()


def _last(response: ModelResponse) -> AIMessage | None:
    last = response.result[-1] if response.result else None
    return last if isinstance(last, AIMessage) else None


class ResearchFirst(AgentMiddleware):
    def __init__(self, whole_read_chars: int = WHOLE_READ_CHARS) -> None:
        super().__init__()
        self._whole = whole_read_chars

    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        messages = list(request.messages)
        question, turn = _this_turn(messages)
        if question is not None and keyword(question) and not researched(turn):
            step = doc_step(request.state, messages, self._whole, question)
            if step is not None:
                log.info("research_first doc tool=%s", step.tool_calls[0]["name"])
                return ModelResponse(result=[step])
            log.info("research_first task")
            return ModelResponse(result=[research_call(question, law_lines(request.state))])
        response = await handler(request)
        final = _last(response)
        if final is None or final.tool_calls or visible(final):
            return response
        log.info("research_first empty_answer retry")
        retry = request.override(system_message=append_system(request.system_message, EMPTY_ANSWER_NOTE))
        return await handler(retry)
