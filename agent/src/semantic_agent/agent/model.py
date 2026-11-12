import asyncio
import logging
import os
from collections.abc import AsyncIterator
from typing import Any

import httpx
from langchain_core.language_models import BaseChatModel
from langchain_core.outputs import ChatGenerationChunk
from langchain_google_genai import ChatGoogleGenerativeAI
from langchain_openai import ChatOpenAI
from pydantic import Field

from semantic_agent.agent.finish import FinishCheck
from semantic_agent.agent.model_errors import ModelConfigError, RetryPolicy
from semantic_agent.agent.model_transport import ModelTransport
from semantic_agent.agent.repeat import RepeatCut
from semantic_agent.config import Settings

log = logging.getLogger(__name__)

VERTEX_API_VERSION = "v1"
SDK_ATTEMPTS = 1
GOOGLE_ENV = (
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_CLOUD_PROJECT",
    "GOOGLE_CLOUD_LOCATION",
    "GOOGLE_APPLICATION_CREDENTIALS",
)


class VertexChat(ChatGoogleGenerativeAI):
    transport: Any = Field(default=None, exclude=True, repr=False)

    async def _astream(self, *args: Any, **kwargs: Any) -> AsyncIterator[ChatGenerationChunk]:
        owner = asyncio.current_task()
        try:
            async for chunk in super()._astream(*args, **kwargs):
                yield chunk
        finally:
            if isinstance(self.transport, ModelTransport):
                await self.transport.close_left(owner)


def build_model(settings: Settings, transport: ModelTransport | None = None) -> BaseChatModel:
    if settings.llm_provider == "vertex":
        return vertex_model(settings, transport or vertex_transport(settings))
    return openai_model(settings)


def llm_timeout(settings: Settings) -> httpx.Timeout:
    return httpx.Timeout(settings.llm_read_timeout_s, connect=settings.llm_connect_timeout_s)


def openai_model(settings: Settings) -> ChatOpenAI:
    return ChatOpenAI(
        base_url=settings.llm_base_url,
        api_key=settings.llm_api_key,
        model=settings.llm_model,
        temperature=0,
        max_tokens=settings.llm_max_tokens,
        timeout=llm_timeout(settings),
        max_retries=settings.llm_max_retries,
        stream_usage=True,
        callbacks=[RepeatCut()],
        extra_body={"priority": 0},
    )


def vertex_transport(settings: Settings, inner: Any = None, **options: Any) -> ModelTransport:
    return ModelTransport(
        RetryPolicy(retries=settings.llm_max_retries),
        llm_timeout(settings),
        settings.vertex_max_concurrency,
        inner=inner,
        **options,
    )


def _refuse_ambient_google(settings: Settings) -> None:
    reasons = []
    if settings.vertex_project or settings.vertex_location:
        reasons.append(
            "VERTEX_PROJECT·VERTEX_LOCATION이 있으면 langchain-google-genai 4.4.0이 API 키를 버리고 "
            "기본 사용자 인증(ADC)으로 부릅니다. API 키로 부르려면 두 값을 비우십시오"
        )
    present = [name for name in GOOGLE_ENV if os.environ.get(name)]
    if present:
        reasons.append(
            f"환경변수 {', '.join(present)}가 있으면 라이브러리가 VERTEX_API_KEY 대신 그 값이나 이 기계의 인증 정보를 쓸 수 있습니다. "
            "이 환경변수들을 지우십시오"
        )
    if reasons:
        raise ModelConfigError("Vertex AI 모델을 API 키로 만들 수 없습니다: " + " / ".join(reasons))


def _require_key_auth(model: ChatGoogleGenerativeAI, key: str) -> None:
    api = getattr(model.client, "_api_client", None)
    problems = []
    if api is None or not getattr(api, "vertexai", False):
        problems.append("Vertex AI 백엔드가 아닙니다")
    if api is not None and getattr(api, "api_key", None) != key:
        problems.append("설정한 API 키가 요청 헤더에 쓰이지 않습니다")
    if api is not None and (getattr(api, "project", None) or getattr(api, "location", None)):
        problems.append("프로젝트·지역 경로로 부르도록 잡혔습니다")
    if problems:
        raise ModelConfigError("Vertex AI 모델 연결 설정이 의도와 다릅니다: " + ", ".join(problems))


def vertex_model(settings: Settings, transport: ModelTransport) -> VertexChat:
    _refuse_ambient_google(settings)
    thinking = {
        name: value
        for name, value in (
            ("thinking_level", settings.vertex_thinking_level),
            ("thinking_budget", settings.vertex_thinking_budget),
        )
        if value is not None
    }
    model = VertexChat(
        transport=transport,
        model=settings.vertex_model,
        api_key=settings.vertex_api_key,
        vertexai=True,
        project=None,
        location=None,
        api_version=VERTEX_API_VERSION,
        temperature=0,
        max_tokens=settings.llm_max_tokens,
        max_retries=SDK_ATTEMPTS,
        timeout=None,
        client_args={"transport": transport, "trust_env": False},
        callbacks=[RepeatCut(), FinishCheck(settings.llm_max_tokens)],
        **thinking,
    )
    _require_key_auth(model, settings.vertex_api_key.get_secret_value())
    log.info(
        "vertex model ready model=%s api_version=%s max_tokens=%d retries=%d limit=%d thinking=%s",
        settings.vertex_model,
        VERTEX_API_VERSION,
        settings.llm_max_tokens,
        settings.llm_max_retries,
        transport.limit,
        sorted(thinking.items()),
    )
    return model
