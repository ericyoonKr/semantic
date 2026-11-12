import json
import uuid
from collections.abc import Iterator
from typing import Any

from langchain_core.language_models import BaseChatModel
from langchain_core.messages import AIMessage, ToolMessage
from langchain_core.outputs import ChatGeneration, ChatResult
from langgraph.checkpoint.memory import InMemorySaver
from langgraph.store.memory import InMemoryStore
from pydantic import Field

from semantic_agent.agent.build import build_agent, load_catalogs
from semantic_agent.agent.research import ResearchOptions
from semantic_agent.tools.client import ToolClient
from semantic_agent.tools.fake import FakeClient


def _names(tools: Any) -> list[str]:
    return [t.get("name", "") if isinstance(t, dict) else getattr(t, "name", "") for t in tools]


class ScriptedToolModel(BaseChatModel):
    script: Iterator[AIMessage]
    calls: int = 0
    seen: list[list[Any]] = Field(default_factory=list)
    bound: list[list[str]] = Field(default_factory=list)

    @property
    def _llm_type(self) -> str:
        return "scripted-tool-model"

    def _generate(self, messages: Any, stop: Any = None, run_manager: Any = None, **kwargs: Any) -> ChatResult:
        self.calls += 1
        self.seen.append(list(messages))
        return ChatResult(generations=[ChatGeneration(message=next(self.script))])

    def bind_tools(self, tools: Any, **kwargs: Any) -> "ScriptedToolModel":
        self.bound.append(_names(tools))
        return self


def call(name: str, args: dict[str, Any], call_id: str) -> AIMessage:
    return AIMessage(content="", tool_calls=[{"name": name, "args": args, "id": call_id}])


def delegate(subagent: str, description: str, call_id: str) -> AIMessage:
    return call("task", {"description": description, "subagent_type": subagent}, call_id)


def answer(found: list[dict[str, Any]], not_found: list[str] | None = None) -> AIMessage:
    return AIMessage(content=json.dumps({"found": found, "not_found": not_found or []}, ensure_ascii=False))


def tool_messages(messages: list[Any]) -> list[ToolMessage]:
    return [m for m in messages if isinstance(m, ToolMessage)]


async def make_agent(
    script: list[AIMessage] | None = None,
    client: ToolClient | None = None,
    model: BaseChatModel | None = None,
    research: ResearchOptions | None = None,
) -> tuple[Any, Any, Any]:
    use_client = client or FakeClient()
    catalogs = await load_catalogs(use_client)
    if isinstance(use_client, FakeClient):
        use_client.calls.clear()
    use_model = model or ScriptedToolModel(script=iter(script or []))
    agent = build_agent(use_model, use_client, catalogs, InMemorySaver(), InMemoryStore(), research)
    return agent, use_model, use_client


def run_config(recursion_limit: int = 60, request_id: str = "test") -> dict[str, Any]:
    return {
        "configurable": {"thread_id": str(uuid.uuid4()), "user_id": str(uuid.uuid4()), "request_id": request_id},
        "recursion_limit": recursion_limit,
    }
