import time
from collections.abc import Awaitable, Callable, Sequence
from contextvars import ContextVar
from typing import Any

from deepagents import FilesystemPermission, SubAgent
from deepagents.backends.protocol import BackendProtocol
from deepagents.middleware.filesystem import FilesystemMiddleware
from langchain.agents.middleware import AgentMiddleware, ModelRequest, ModelResponse
from langchain_core.messages import AIMessage
from langchain_core.tools import BaseTool

from semantic_agent.agent.context import append_system
from semantic_agent.agent.guards import ToolGuard
from semantic_agent.agent.prompts import DOC_READER_PROMPT, KG_EXPLORER_PROMPT, RESEARCHER_PROMPT, WRAP_UP_PROMPT

RESEARCHER = "researcher"
DOC_READER = "doc_reader"
KG_EXPLORER = "kg_explorer"
SUBAGENT_NAMES = (RESEARCHER, DOC_READER, KG_EXPLORER)

SUB_STEP_LIMIT = 20
SUB_DEADLINE_S = 120.0
LLM_CALL_S = 45.0
LIMIT_MARK = "제한에 걸림"

SUB_DEADLINE: ContextVar[float | None] = ContextVar("sub_deadline", default=None)

NO_FILES = [FilesystemPermission(operations=["read", "write"], paths=["/**"], mode="deny")]
UPLOADS_READ_ONLY = [
    FilesystemPermission(operations=["read"], paths=["/uploads", "/uploads/**"], mode="allow"),
    FilesystemPermission(operations=["read", "write"], paths=["/**"], mode="deny"),
]
DOC_READER_FILE_TOOLS = ["ls", "grep", "read_file"]

RESEARCHER_DESCRIPTION = (
    "법령·판례를 검색하고 읽으며, 판례가 인용한 조문과 조문을 인용한 판례를 따라간다. "
    "찾은 것을 주장·출처·원문 인용 목록으로, 찾지 못한 것을 목록으로 돌려준다."
)
DOC_READER_DESCRIPTION = (
    "업로드 문서(/uploads/)에서 질문에 맞는 대목을 찾아 읽는다. 읽기만 한다. "
    "찾은 것을 주장·출처(파일·쪽)·원문 인용 목록으로, 찾지 못한 것을 목록으로 돌려준다."
)
KG_EXPLORER_DESCRIPTION = (
    "지식그래프에서 개념·위임 관계를 여러 단계 따라간다. "
    "찾은 것을 주장·출처(그래프·노드 번호)·근거 인용 목록으로, 찾지 못한 것을 목록으로 돌려준다."
)


def _tool_name(tool: Any) -> str:
    if isinstance(tool, dict):
        return str(tool.get("name") or tool.get("function", {}).get("name", ""))
    return str(getattr(tool, "name", ""))


class ToolAllowlist(AgentMiddleware):
    def __init__(self, names: Sequence[str]) -> None:
        super().__init__()
        self._names = frozenset(names)

    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        tools = [t for t in request.tools if _tool_name(t) in self._names]
        return await handler(request.override(tools=tools))


def _close(response: Any, mark: str) -> ModelResponse:
    messages = list(response.result) if isinstance(response, ModelResponse) else [response]
    last = messages[-1]
    text = last.text if isinstance(last, AIMessage) else str(getattr(last, "content", ""))
    closed = AIMessage(content=f"{text}\n{mark}" if text else mark, id=getattr(last, "id", None))
    structured = response.structured_response if isinstance(response, ModelResponse) else None
    return ModelResponse(result=[*messages[:-1], closed], structured_response=structured)


class SubagentLimit(AgentMiddleware):
    def __init__(
        self,
        steps: int = SUB_STEP_LIMIT,
        deadline_s: float = SUB_DEADLINE_S,
        reserve_s: float = LLM_CALL_S,
    ) -> None:
        super().__init__()
        self._steps = steps
        self._deadline_s = deadline_s
        self._reserve_s = reserve_s

    def _reason(self, state: Any) -> str | None:
        step = 1 + sum(1 for m in (state or {}).get("messages", []) if isinstance(m, AIMessage))
        if step >= self._steps:
            return f"단계 상한 {self._steps}"
        deadline = SUB_DEADLINE.get()
        if deadline is not None and deadline - time.monotonic() <= self._reserve_s:
            return f"시간 상한 {int(self._deadline_s)}초"
        return None

    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        reason = self._reason(request.state)
        if reason is None:
            return await handler(request)
        last = request.override(tools=[], system_message=append_system(request.system_message, WRAP_UP_PROMPT))
        return _close(await handler(last), f"{LIMIT_MARK}: {reason}")


def _spec(
    name: str,
    description: str,
    prompt: str,
    tools: list[BaseTool],
    file_tools: list[str],
    permissions: list[FilesystemPermission],
    backend: BackendProtocol,
) -> SubAgent:
    visible = [t.name for t in tools] + file_tools
    return {
        "name": name,
        "description": description,
        "system_prompt": prompt,
        "tools": tools,
        "middleware": [
            FilesystemMiddleware(backend=backend, tools=file_tools or ["read_file"], _permissions=permissions),
            ToolAllowlist(visible),
            ToolGuard(),
            SubagentLimit(),
        ],
        "permissions": permissions,
    }


def make_subagents(
    backend: BackendProtocol,
    search_tools: list[BaseTool],
    kg_tools: list[BaseTool],
) -> list[SubAgent]:
    return [
        _spec(RESEARCHER, RESEARCHER_DESCRIPTION, RESEARCHER_PROMPT, search_tools, [], NO_FILES, backend),
        _spec(DOC_READER, DOC_READER_DESCRIPTION, DOC_READER_PROMPT, [], DOC_READER_FILE_TOOLS, UPLOADS_READ_ONLY, backend),
        _spec(KG_EXPLORER, KG_EXPLORER_DESCRIPTION, KG_EXPLORER_PROMPT, kg_tools, [], NO_FILES, backend),
    ]
