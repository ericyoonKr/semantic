from collections.abc import Awaitable, Callable
from typing import Any

from langchain.agents.middleware import AgentMiddleware, ModelRequest, ModelResponse

UNUSED_TOOLS: frozenset[str] = frozenset({"ls", "write_file", "edit_file", "delete", "glob", "grep", "execute", "write_todos"})


def trimmed(tools: list[Any], excluded: frozenset[str]) -> list[Any]:
    return [t for t in tools if getattr(t, "name", t.get("name") if isinstance(t, dict) else None) not in excluded]


class ToolTrim(AgentMiddleware):
    def __init__(self, excluded: frozenset[str] = UNUSED_TOOLS) -> None:
        super().__init__()
        self._excluded = excluded

    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        tools = list(getattr(request, "tools", None) or [])
        kept = trimmed(tools, self._excluded)
        if len(kept) != len(tools):
            request = request.override(tools=kept)
        return await handler(request)
