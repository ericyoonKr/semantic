import asyncio
import json
from collections.abc import Callable
from typing import Any

import httpx
import pytest
from pydantic import SecretStr

from semantic_agent.agent.model import GOOGLE_ENV, build_model, gemini_transport
from semantic_agent.agent.model_transport import ModelTransport
from semantic_agent.config import Settings

KEY = "vk-test-0123456789"
MODEL_ID = "gemini-test-model"
GENERATE_URL = f"https://generativelanguage.googleapis.com/v1beta/models/{MODEL_ID}:generateContent"
STREAM_URL = f"https://generativelanguage.googleapis.com/v1beta/models/{MODEL_ID}:streamGenerateContent?alt=sse"


def candidate(text: str = "답", finish: str | None = "STOP", parts: list[dict[str, Any]] | None = None) -> dict[str, Any]:
    item: dict[str, Any] = {"content": {"role": "model", "parts": parts or [{"text": text}]}}
    if finish:
        item["finishReason"] = finish
    return item


def body(*candidates: dict[str, Any], usage: tuple[int, int, int] = (3, 1, 4), **extra: Any) -> dict[str, Any]:
    data: dict[str, Any] = {
        "candidates": list(candidates),
        "usageMetadata": {"promptTokenCount": usage[0], "candidatesTokenCount": usage[1], "totalTokenCount": usage[2]},
        "modelVersion": MODEL_ID,
    }
    data.update(extra)
    return data


def wire(status: int, data: bytes, kind: str = "application/json", **headers: str) -> httpx.Response:
    return httpx.Response(status, headers={"content-type": kind, **headers}, stream=httpx.ByteStream(data))


def ok(data: dict[str, Any]) -> httpx.Response:
    return wire(200, json.dumps(data).encode())


def sse(*chunks: dict[str, Any]) -> httpx.Response:
    text = "".join(f"data: {json.dumps(chunk)}\r\n\r\n" for chunk in chunks)
    return wire(200, text.encode(), "text/event-stream")


def google_failure(code: int, status: str, message: str = "", details: list[dict[str, Any]] | None = None, **headers: str) -> httpx.Response:
    error = {"error": {"code": code, "status": status, "message": message, "details": details or []}}
    return wire(code, json.dumps(error).encode(), **headers)


class FakeGemini:
    def __init__(self, *replies: httpx.Response | Exception | Callable[[httpx.Request], Any]) -> None:
        self.replies = list(replies)
        self.requests: list[httpx.Request] = []

    def _next(self, request: httpx.Request) -> Any:
        self.requests.append(request)
        reply = self.replies.pop(0) if len(self.replies) > 1 else self.replies[0]
        if isinstance(reply, httpx.Response):
            return httpx.Response(reply.status_code, headers=reply.headers, stream=httpx.ByteStream(b"".join(reply.stream)))
        return reply

    def handler(self, request: httpx.Request) -> httpx.Response:
        reply = self._next(request)
        if isinstance(reply, Exception):
            raise reply
        return reply(request) if callable(reply) else reply

    async def ahandler(self, request: httpx.Request) -> httpx.Response:
        reply = self._next(request)
        if isinstance(reply, Exception):
            raise reply
        if callable(reply):
            reply = reply(request)
            if asyncio.iscoroutine(reply):
                reply = await reply
        return reply

    def json(self, index: int = -1) -> dict[str, Any]:
        return json.loads(self.requests[index].content)


def gemini_settings(**kwargs: Any) -> Settings:
    values: dict[str, Any] = {"llm_provider": "gemini", "vertex_api_key": SecretStr(KEY), "vertex_model": MODEL_ID}
    values.update(kwargs)
    return Settings(**values)


class NoSleep:
    def __init__(self) -> None:
        self.waits: list[float] = []

    async def __call__(self, seconds: float) -> None:
        self.waits.append(seconds)

    def sync(self, seconds: float) -> None:
        self.waits.append(seconds)


def fake_model(fake: FakeGemini, settings: Settings | None = None, *, use_async: bool = True, limit: int | None = None) -> tuple[Any, ModelTransport, NoSleep]:
    settings = settings or gemini_settings()
    if limit is not None:
        settings = settings.model_copy(update={"vertex_max_concurrency": limit})
    nap = NoSleep()
    inner = httpx.MockTransport(fake.ahandler if use_async else fake.handler)
    transport = gemini_transport(settings, inner, asleep=nap, sleep=nap.sync)
    return build_model(settings, transport), transport, nap


AMBIENT_ENV = (*GOOGLE_ENV, "GOOGLE_GENAI_USE_VERTEXAI", "GOOGLE_GENAI_USE_ENTERPRISE", "LANGSMITH_GATEWAY")


@pytest.fixture
def clean_google_env(monkeypatch: pytest.MonkeyPatch) -> None:
    for name in AMBIENT_ENV:
        monkeypatch.delenv(name, raising=False)
