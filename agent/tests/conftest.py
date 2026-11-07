import uuid
from collections.abc import Iterator
from typing import Any

from langchain_core.language_models import BaseChatModel
from langchain_core.messages import AIMessage
from langchain_core.outputs import ChatGeneration, ChatResult
from langgraph.checkpoint.memory import InMemorySaver
from langgraph.store.memory import InMemoryStore

from semantic_agent.agent.build import build_agent, load_catalogs
from semantic_agent.tools.fake import FakeClient


class ScriptedToolModel(BaseChatModel):
    script: Iterator[AIMessage]
    calls: int = 0

    @property
    def _llm_type(self) -> str:
        return "scripted-tool-model"

    def _generate(self, messages: Any, stop: Any = None, run_manager: Any = None, **kwargs: Any) -> ChatResult:
        self.calls += 1
        return ChatResult(generations=[ChatGeneration(message=next(self.script))])

    def bind_tools(self, tools: Any, **kwargs: Any) -> "ScriptedToolModel":
        return self


def call(name: str, args: dict[str, Any], call_id: str) -> AIMessage:
    return AIMessage(content="", tool_calls=[{"name": name, "args": args, "id": call_id}])


async def make_agent(script: list[AIMessage]) -> tuple[Any, ScriptedToolModel, FakeClient]:
    client = FakeClient()
    catalogs = await load_catalogs(client)
    client.calls.clear()
    model = ScriptedToolModel(script=iter(script))
    agent = build_agent(model, client, catalogs, InMemorySaver(), InMemoryStore())
    return agent, model, client


def run_config(recursion_limit: int = 60) -> dict[str, Any]:
    return {
        "configurable": {"thread_id": str(uuid.uuid4()), "user_id": str(uuid.uuid4()), "request_id": "test"},
        "recursion_limit": recursion_limit,
    }
