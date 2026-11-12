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
from semantic_agent.config import GEMINI_PROVIDERS, Settings

log = logging.getLogger(__name__)

GEMINI_BASE_URL = "https://generativelanguage.googleapis.com/"
SDK_ATTEMPTS = 1
GOOGLE_ENV = (
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_CLOUD_PROJECT",
    "GOOGLE_CLOUD_LOCATION",
    "GOOGLE_APPLICATION_CREDENTIALS",
)


class GeminiChat(ChatGoogleGenerativeAI):
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
    if settings.llm_provider in GEMINI_PROVIDERS:
        return gemini_model(settings, transport or gemini_transport(settings))
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


def gemini_transport(settings: Settings, inner: Any = None, **options: Any) -> ModelTransport:
    return ModelTransport(
        RetryPolicy(retries=settings.llm_max_retries),
        llm_timeout(settings),
        settings.vertex_max_concurrency,
        inner=inner,
        **options,
    )


def _refuse_ambient_google() -> None:
    present = [name for name in GOOGLE_ENV if os.environ.get(name)]
    if present:
        raise ModelConfigError(
            f"Gemini API 모델을 만들지 않습니다: 환경변수 {', '.join(present)}가 있습니다. "
            "VERTEX_API_KEY 말고 다른 키나 이 기계의 인증 정보가 섞이지 않게 이 환경변수들을 지우십시오"
        )


def _require_key_auth(model: ChatGoogleGenerativeAI, key: str) -> None:
    api = getattr(model.client, "_api_client", None)
    if api is None:
        raise ModelConfigError("Gemini API 모델 연결 설정이 의도와 다릅니다: 클라이언트가 없습니다")
    problems = []
    if getattr(api, "vertexai", None):
        problems.append(
            "Vertex AI 백엔드로 잡혔습니다(환경변수 GOOGLE_GENAI_USE_VERTEXAI 또는 GOOGLE_GENAI_USE_ENTERPRISE를 확인하십시오)"
        )
    if getattr(api, "api_key", None) != key:
        problems.append("설정한 API 키가 요청 헤더에 쓰이지 않습니다")
    base_url = getattr(getattr(api, "_http_options", None), "base_url", None)
    if base_url != GEMINI_BASE_URL:
        problems.append(f"요청 주소가 {GEMINI_BASE_URL}가 아닙니다({base_url})")
    if problems:
        raise ModelConfigError("Gemini API 모델 연결 설정이 의도와 다릅니다: " + ", ".join(problems))


def gemini_model(settings: Settings, transport: ModelTransport) -> GeminiChat:
    _refuse_ambient_google()
    thinking = {
        name: value
        for name, value in (
            ("thinking_level", settings.vertex_thinking_level),
            ("thinking_budget", settings.vertex_thinking_budget),
        )
        if value is not None
    }
    model = GeminiChat(
        transport=transport,
        model=settings.vertex_model,
        api_key=settings.vertex_api_key,
        vertexai=False,
        base_url=GEMINI_BASE_URL,
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
        "gemini model ready model=%s provider=%s max_tokens=%d retries=%d limit=%d thinking=%s",
        settings.vertex_model,
        settings.llm_provider,
        settings.llm_max_tokens,
        settings.llm_max_retries,
        transport.limit,
        sorted(thinking.items()),
    )
    return model
