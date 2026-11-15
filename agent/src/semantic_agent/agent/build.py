from dataclasses import dataclass, field
from typing import Any

from deepagents import (
    FilesystemPermission,
    GeneralPurposeSubagentProfile,
    HarnessProfile,
    create_deep_agent,
    register_harness_profile,
)
from deepagents.backends import CompositeBackend, StateBackend, StoreBackend
from deepagents.backends.protocol import BackendProtocol
from deepagents.middleware import SummarizationMiddleware
from langchain_core.language_models import BaseChatModel
from langchain_core.tools import BaseTool
from langgraph.checkpoint.base import BaseCheckpointSaver
from langgraph.config import get_config
from langgraph.store.base import BaseStore

from semantic_agent.agent.budget import QuestionBudget
from semantic_agent.agent.tooltrim import ToolTrim
from semantic_agent.agent.context import ContextMiddleware
from semantic_agent.agent.delegation import SubagentGate
from semantic_agent.agent.guards import ScopeGuard, ToolGuard
from semantic_agent.agent.intent import IntentScope
from semantic_agent.agent.judge_route import JudgeRoute
from semantic_agent.agent.progress import ProgressMiddleware
from semantic_agent.agent.prompts import SYSTEM_PROMPT
from semantic_agent.agent.research import VERSION_TOOLS, ResearchOptions
from semantic_agent.funcall.first_step import FirstStep
from semantic_agent.agent.subagents import SUBAGENT_NAMES, make_subagents
from semantic_agent.memory.store import prefs_namespace
from semantic_agent.tools.annex import make_annex_tools
from semantic_agent.tools.client import ToolClient
from semantic_agent.tools.document import make_document_tool
from semantic_agent.tools.judge import format_judge_catalog, make_judge_tool
from semantic_agent.tools.kg import catalog_hashes, format_kg_catalog, make_kg_tools
from semantic_agent.tools.kg_state import make_kg_state_tool, make_kg_tree_tool
from semantic_agent.tools.search import make_search_tools
from semantic_agent.tools.versions import make_version_tools

MAIN_AGENT = "main"

PERMISSIONS = [
    FilesystemPermission(operations=["write"], paths=["/uploads/**"], mode="deny"),
    FilesystemPermission(operations=["write"], paths=["/memory/session.md", "/prefs/answer_style.md"], mode="allow"),
    FilesystemPermission(operations=["write"], paths=["/**"], mode="deny"),
]


@dataclass(frozen=True)
class Catalogs:
    judge: str
    kg: str
    routes: dict[str, list[str]] = field(default_factory=dict)
    kg_hashes: dict[str, str] = field(default_factory=dict)


def claim_routes(data: Any) -> dict[str, list[str]]:
    routes: dict[str, list[str]] = {}
    for item in data if isinstance(data, list) else []:
        if not isinstance(item, dict):
            continue
        claim_type = str(item.get("claim_type") or "").strip()
        units = routes.setdefault(claim_type, []) if claim_type else []
        for evidence in item.get("evidence") or []:
            if not isinstance(evidence, dict):
                continue
            unit = " ".join(str(evidence.get(k) or "").strip() for k in ("law", "unit")).strip()
            if unit and unit not in units:
                units.append(unit)
    return {k: v for k, v in routes.items() if v}


async def load_catalogs(client: ToolClient) -> Catalogs:
    judge_data, judge_err = await client.fetch("GET", "/catalog", kind="search", fail="카탈로그 읽기 실패")
    kg_data, kg_err = await client.fetch("GET", "/kg/catalog", kind="search", fail="카탈로그 읽기 실패")
    return Catalogs(
        judge=judge_err or format_judge_catalog(judge_data),
        kg=kg_err or format_kg_catalog(kg_data),
        routes={} if judge_err else claim_routes(judge_data),
        kg_hashes={} if kg_err else catalog_hashes(kg_data),
    )


def _prefs_namespace(_runtime: Any) -> tuple[str, ...]:
    user_id = get_config().get("configurable", {}).get("user_id")
    if not user_id:
        raise RuntimeError("user_id가 없는 실행에서는 사용자 설정을 쓸 수 없습니다")
    return prefs_namespace(str(user_id))


SUMMARY_TRIGGER_TOKENS = 20_000
SUMMARY_KEEP_MESSAGES = 8


def make_summarization(model: BaseChatModel, backend: BackendProtocol) -> SummarizationMiddleware:
    return SummarizationMiddleware(
        model, backend=backend, trigger=("tokens", SUMMARY_TRIGGER_TOKENS), keep=("messages", SUMMARY_KEEP_MESSAGES)
    )


def _disable_general_purpose(model: BaseChatModel) -> None:
    provider = model._get_ls_params().get("ls_provider")
    if not provider:
        raise RuntimeError("모델의 제공자 이름을 알 수 없어 general-purpose 하위 에이전트를 끌 수 없습니다")
    register_harness_profile(
        str(provider),
        HarnessProfile(general_purpose_subagent=GeneralPurposeSubagentProfile(enabled=False)),
    )


def researcher_tools(client: ToolClient, research: ResearchOptions) -> list[BaseTool]:
    tools = [*make_search_tools(client, scoped=research.steps), *make_annex_tools(client), *make_kg_tools(client)]
    if research.steps:
        tools += [t for t in make_version_tools(client) if t.name in VERSION_TOOLS]
    return tools


def build_agent(
    model: BaseChatModel,
    client: ToolClient,
    catalogs: Catalogs,
    checkpointer: BaseCheckpointSaver,
    store: BaseStore,
    research: ResearchOptions | None = None,
) -> Any:
    research = research or ResearchOptions()
    _disable_general_purpose(model)
    backend = CompositeBackend(
        default=StateBackend(),
        routes={"/prefs/": StoreBackend(namespace=_prefs_namespace)},
    )
    subagents = make_subagents(
        backend, researcher_tools(client, research), research.researcher_prompt(), research.deadline_s, client
    )
    return create_deep_agent(
        model=model,
        tools=[
            make_judge_tool(client, catalogs.judge),
            *make_kg_tools(client),
            make_kg_state_tool(client),
            make_kg_tree_tool(client),
            *make_version_tools(client),
            make_document_tool(client),
        ],
        system_prompt=SYSTEM_PROMPT,
        middleware=[
            make_summarization(model, backend),
            *([FirstStep(client)] if research.first else []),
            QuestionBudget(),
            ToolTrim(),
            ScopeGuard(),
            ContextMiddleware(catalogs.judge, catalogs.kg),
            JudgeRoute(catalogs.routes.keys()),
            ProgressMiddleware(catalogs.kg_hashes),
            ToolGuard(),
            SubagentGate(
                SUBAGENT_NAMES,
                deadline_s=research.deadline_s,
                routes=catalogs.routes,
                scope=IntentScope(client) if research.intent_scope else None,
                client=client,
            ),
        ],
        subagents=subagents,
        permissions=PERMISSIONS,
        backend=backend,
        checkpointer=checkpointer,
        store=store,
        name=MAIN_AGENT,
    )
