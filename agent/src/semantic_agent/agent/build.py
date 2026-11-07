from dataclasses import dataclass
from typing import Any

from deepagents import (
    FilesystemPermission,
    GeneralPurposeSubagentProfile,
    HarnessProfile,
    SubAgent,
    create_deep_agent,
    register_harness_profile,
)
from deepagents.backends import CompositeBackend, StateBackend, StoreBackend
from langchain_core.language_models import BaseChatModel
from langchain_core.tools import BaseTool
from langchain_openai import ChatOpenAI
from langgraph.checkpoint.base import BaseCheckpointSaver
from langgraph.config import get_config
from langgraph.store.base import BaseStore

from semantic_agent.agent.context import ContextMiddleware
from semantic_agent.agent.guards import ScopeGuard, ToolGuard
from semantic_agent.agent.prompts import KG_EXPLORER_PROMPT, SYSTEM_PROMPT
from semantic_agent.config import Settings
from semantic_agent.memory.store import prefs_namespace
from semantic_agent.tools.client import ToolClient
from semantic_agent.tools.judge import format_judge_catalog, make_judge_tool
from semantic_agent.tools.kg import format_kg_catalog, make_kg_tools
from semantic_agent.tools.search import make_search_tools

MAIN_AGENT = "main"
KG_EXPLORER = "kg_explorer"

PERMISSIONS = [
    FilesystemPermission(operations=["write"], paths=["/uploads/**"], mode="deny"),
    FilesystemPermission(operations=["write"], paths=["/memory/session.md", "/prefs/answer_style.md"], mode="allow"),
    FilesystemPermission(operations=["write"], paths=["/**"], mode="deny"),
]


@dataclass(frozen=True)
class Catalogs:
    judge: str
    kg: str


async def load_catalogs(client: ToolClient) -> Catalogs:
    judge_data, judge_err = await client.fetch("GET", "/catalog", kind="search", fail="카탈로그 읽기 실패")
    kg_data, kg_err = await client.fetch("GET", "/kg/catalog", kind="search", fail="카탈로그 읽기 실패")
    return Catalogs(
        judge=judge_err or format_judge_catalog(judge_data),
        kg=kg_err or format_kg_catalog(kg_data),
    )


def build_model(settings: Settings) -> ChatOpenAI:
    return ChatOpenAI(
        base_url=settings.llm_base_url,
        api_key=settings.llm_api_key,
        model=settings.llm_model,
        temperature=0,
        extra_body={"priority": 0},
    )


def _prefs_namespace(_runtime: Any) -> tuple[str, ...]:
    user_id = get_config().get("configurable", {}).get("user_id")
    if not user_id:
        raise RuntimeError("user_id가 없는 실행에서는 사용자 설정을 쓸 수 없습니다")
    return prefs_namespace(str(user_id))


def _disable_general_purpose(model: BaseChatModel) -> None:
    provider = model._get_ls_params().get("ls_provider")
    if not provider:
        raise RuntimeError("모델의 제공자 이름을 알 수 없어 general-purpose 하위 에이전트를 끌 수 없습니다")
    register_harness_profile(
        str(provider),
        HarnessProfile(general_purpose_subagent=GeneralPurposeSubagentProfile(enabled=False)),
    )


def build_tools(client: ToolClient, catalogs: Catalogs) -> tuple[list[BaseTool], list[BaseTool]]:
    kg_tools = make_kg_tools(client)
    main_tools = [*make_search_tools(client), *kg_tools, make_judge_tool(client, catalogs.judge)]
    return main_tools, kg_tools


def build_agent(
    model: BaseChatModel,
    client: ToolClient,
    catalogs: Catalogs,
    checkpointer: BaseCheckpointSaver,
    store: BaseStore,
) -> Any:
    _disable_general_purpose(model)
    main_tools, kg_tools = build_tools(client, catalogs)
    backend = CompositeBackend(
        default=StateBackend(),
        routes={"/prefs/": StoreBackend(namespace=_prefs_namespace)},
    )
    kg_explorer: SubAgent = {
        "name": KG_EXPLORER,
        "description": "지식그래프를 여러 단계(예: 위임망 3단계 넘게) 따라가야 할 때만 쓴다. 답변에 경로의 노드 번호를 모두 적는다.",
        "system_prompt": KG_EXPLORER_PROMPT,
        "tools": kg_tools,
        "middleware": [ToolGuard()],
        "permissions": PERMISSIONS,
    }
    return create_deep_agent(
        model=model,
        tools=main_tools,
        system_prompt=SYSTEM_PROMPT,
        middleware=[ScopeGuard(), ContextMiddleware(catalogs.judge, catalogs.kg), ToolGuard()],
        subagents=[kg_explorer],
        permissions=PERMISSIONS,
        backend=backend,
        checkpointer=checkpointer,
        store=store,
        name=MAIN_AGENT,
    )
