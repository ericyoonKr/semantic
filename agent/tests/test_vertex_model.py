import asyncio

import httpx
import pytest
from deepagents.profiles.harness.harness_profiles import _harness_profile_for_model
from langchain_core.tools import tool
from langchain_google_genai import ChatGoogleGenerativeAI
from langchain_openai import ChatOpenAI
from pydantic import SecretStr
from vertex_fake import (
    EXPRESS_URL,
    KEY,
    MODEL_ID,
    STREAM_URL,
    FakeVertex,
    body,
    candidate,
    clean_google_env,
    fake_model,
    google_failure,
    ok,
    sse,
    vertex_settings,
)

from semantic_agent.agent.budget import BudgetExceeded
from semantic_agent.agent.build import _disable_general_purpose
from semantic_agent.agent.model import build_model
from semantic_agent.agent.model_errors import ModelAuthError, ModelConfigError, ModelOutputError
from semantic_agent.config import Settings

pytestmark = pytest.mark.usefixtures("clean_google_env")
__all__ = ["clean_google_env"]


def test_provider_defaults_to_openai_and_keeps_openai_model() -> None:
    settings = Settings(llm_base_url="http://127.0.0.1:1/v1", llm_api_key=SecretStr("x"), llm_model="m")
    assert settings.llm_provider == "openai"
    assert isinstance(build_model(settings), ChatOpenAI)


def test_provider_and_vertex_values_read_from_env(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("LLM_PROVIDER", "vertex")
    monkeypatch.setenv("VERTEX_API_KEY", KEY)
    monkeypatch.setenv("VERTEX_MODEL", MODEL_ID)
    monkeypatch.setenv("VERTEX_MAX_CONCURRENCY", "3")
    settings = Settings(_env_file=None)
    assert settings.llm_provider == "vertex"
    assert settings.vertex_api_key.get_secret_value() == KEY
    assert settings.vertex_model == MODEL_ID
    assert settings.vertex_max_concurrency == 3
    assert settings.vertex_project == settings.vertex_location == ""
    assert settings.vertex_thinking_level is None and settings.vertex_thinking_budget is None


def test_missing_names_follow_provider() -> None:
    base = {"internal_key": SecretStr("i"), "checkpoint_pg_url": SecretStr("p")}
    assert Settings(**base, llm_provider="vertex").missing() == ["vertex_api_key", "vertex_model"]
    assert Settings(**base, llm_provider="vertex", vertex_api_key=SecretStr(KEY)).missing() == ["vertex_model"]
    assert Settings(**base, llm_provider="vertex", vertex_api_key=SecretStr(KEY), vertex_model=MODEL_ID).missing() == []
    assert Settings(**base, llm_provider="vertex", llm_base_url="u", llm_model="m").missing() == ["vertex_api_key", "vertex_model"]
    assert Settings(**base).missing() == ["llm_base_url", "llm_model"]
    assert Settings(**base, vertex_api_key=SecretStr(KEY), vertex_model=MODEL_ID).missing() == ["llm_base_url", "llm_model"]


def test_unknown_provider_and_bad_concurrency_are_rejected() -> None:
    with pytest.raises(ValueError):
        Settings(llm_provider="gemini")
    with pytest.raises(ValueError):
        Settings(vertex_max_concurrency=0)


def test_key_never_shows_in_settings_or_model_text() -> None:
    settings = vertex_settings()
    model, _, _ = fake_model(FakeVertex(ok(body(candidate()))), settings)
    for text in (repr(settings), str(settings), settings.model_dump_json(), repr(model), str(model)):
        assert KEY not in text


def test_request_goes_to_express_endpoint_with_key_header_and_limits() -> None:
    fake = FakeVertex(ok(body(candidate("안녕"))))
    model, _, _ = fake_model(fake)
    assert isinstance(model, ChatGoogleGenerativeAI)
    reply = asyncio.run(model.ainvoke("질문"))
    request = fake.requests[0]
    assert str(request.url) == EXPRESS_URL
    assert request.headers["x-goog-api-key"] == KEY
    assert KEY not in str(request.url)
    assert request.extensions["timeout"] == {"connect": 5.0, "read": 240.0, "write": 240.0, "pool": 240.0}
    config = fake.json()["generationConfig"]
    assert config["temperature"] == 0
    assert config["maxOutputTokens"] == 8000
    assert "thinkingConfig" not in config
    assert reply.content == "안녕"
    assert reply.usage_metadata["total_tokens"] == 4


def test_stream_reports_usage_and_uses_stream_endpoint() -> None:
    fake = FakeVertex(sse(body(candidate("가", finish=None), usage=(3, 1, 4)), body(candidate("나"), usage=(3, 2, 5))))
    model, _, _ = fake_model(fake)

    async def run() -> list:
        return [chunk async for chunk in model.astream("질문")]

    chunks = asyncio.run(run())
    assert str(fake.requests[0].url) == STREAM_URL
    total = sum((c.usage_metadata or {}).get("total_tokens", 0) for c in chunks)
    assert total == 5
    assert "".join(c.text for c in chunks) == "가나"


def test_repeat_cut_stops_gemini_stream() -> None:
    chunks = [body(candidate("반복되는 문장입니다. " * 4, finish=None)) for _ in range(30)]
    fake = FakeVertex(sse(*chunks, body(candidate("끝"))))
    model, _, _ = fake_model(fake)

    async def run() -> None:
        async for _ in model.astream("질문"):
            pass

    with pytest.raises(BudgetExceeded):
        asyncio.run(run())


def test_bind_tools_sends_declarations_and_reads_calls() -> None:
    @tool(description="법령 찾기")
    def find_law(name: str) -> str:
        return name

    call = candidate(parts=[{"functionCall": {"name": "find_law", "args": {"name": "행정절차법"}}}])
    fake = FakeVertex(ok(body(call)))
    model, _, _ = fake_model(fake)
    reply = asyncio.run(model.bind_tools([find_law]).ainvoke("질문"))
    declared = [d["name"] for t in fake.json()["tools"] for d in t.get("functionDeclarations", [])]
    assert declared == ["find_law"]
    assert [(c["name"], c["args"]) for c in reply.tool_calls] == [("find_law", {"name": "행정절차법"})]


def test_provider_name_turns_off_general_purpose_subagent() -> None:
    model, _, _ = fake_model(FakeVertex(ok(body(candidate()))))
    assert model._get_ls_params()["ls_provider"] == "google_genai"
    _disable_general_purpose(model)
    assert _harness_profile_for_model(model, None).general_purpose_subagent.enabled is False


def test_thinking_is_sent_only_when_set() -> None:
    fake = FakeVertex(ok(body(candidate())))
    model, _, _ = fake_model(fake, vertex_settings(vertex_thinking_level="low"))
    asyncio.run(model.ainvoke("질문"))
    assert fake.json()["generationConfig"]["thinkingConfig"] == {"thinking_level": "LOW"}
    model, _, _ = fake_model(fake, vertex_settings(vertex_thinking_budget=256))
    asyncio.run(model.ainvoke("질문"))
    assert fake.json()["generationConfig"]["thinkingConfig"] == {"thinking_budget": 256}


@pytest.mark.parametrize(
    ("finish", "kind"),
    [
        ("MAX_TOKENS", "출력 한도"),
        ("SAFETY", "출력 안전 차단"),
        ("PROHIBITED_CONTENT", "출력 안전 차단"),
        ("RECITATION", "출력 안전 차단"),
    ],
)
def test_bad_finish_is_an_error_not_an_empty_answer(finish: str, kind: str) -> None:
    fake = FakeVertex(ok(body(candidate("", finish=finish))), sse(body(candidate("일부", finish=finish))))
    model, _, _ = fake_model(fake)
    with pytest.raises(ModelOutputError) as plain:
        asyncio.run(model.ainvoke("질문"))
    assert plain.value.kind == kind

    async def run() -> None:
        async for _ in model.astream("질문"):
            pass

    with pytest.raises(ModelOutputError) as streamed:
        asyncio.run(run())
    assert streamed.value.kind == kind
    assert finish in str(streamed.value)


def test_blocked_prompt_is_an_error() -> None:
    blocked = {"promptFeedback": {"blockReason": "SAFETY"}, "usageMetadata": {"promptTokenCount": 3, "totalTokenCount": 3}}
    fake = FakeVertex(ok(blocked), sse(blocked))
    model, _, _ = fake_model(fake)
    with pytest.raises(ModelOutputError) as plain:
        asyncio.run(model.ainvoke("질문"))
    assert plain.value.kind == "입력 안전 차단"

    async def run() -> None:
        async for _ in model.astream("질문"):
            pass

    with pytest.raises(ModelOutputError) as streamed:
        asyncio.run(run())
    assert streamed.value.kind == "입력 안전 차단"


@pytest.mark.parametrize("finish", ["STOP", "OTHER", "MALFORMED_FUNCTION_CALL"])
def test_other_finish_reasons_pass_through(finish: str) -> None:
    model, _, _ = fake_model(FakeVertex(ok(body(candidate("답", finish=finish)))))
    assert asyncio.run(model.ainvoke("질문")).content == "답"


@pytest.mark.parametrize(
    "settings",
    [
        {"vertex_project": "p"},
        {"vertex_location": "us-central1"},
        {"vertex_project": "p", "vertex_location": "us-central1"},
    ],
)
def test_project_or_location_with_key_is_refused(settings: dict) -> None:
    fake = FakeVertex(ok(body(candidate())))
    with pytest.raises(ModelConfigError) as caught:
        fake_model(fake, vertex_settings(**settings))
    assert "VERTEX_PROJECT" in str(caught.value)
    assert KEY not in str(caught.value)
    assert fake.requests == []


@pytest.mark.parametrize("name", ["GOOGLE_API_KEY", "GEMINI_API_KEY", "GOOGLE_CLOUD_PROJECT", "GOOGLE_CLOUD_LOCATION"])
def test_ambient_google_env_is_refused(monkeypatch: pytest.MonkeyPatch, name: str) -> None:
    monkeypatch.setenv(name, "other-value")
    with pytest.raises(ModelConfigError) as caught:
        fake_model(FakeVertex(ok(body(candidate()))))
    assert name in str(caught.value)
    assert "other-value" not in str(caught.value)


@pytest.mark.parametrize(
    ("name", "expect_key"),
    [("GOOGLE_API_KEY", "other-value"), ("GEMINI_API_KEY", "other-value"), ("GOOGLE_CLOUD_PROJECT", None)],
)
def test_library_really_swaps_key_when_env_is_set(monkeypatch: pytest.MonkeyPatch, name: str, expect_key: str | None) -> None:
    monkeypatch.setenv(name, "other-value")
    model = ChatGoogleGenerativeAI(model=MODEL_ID, api_key=KEY, vertexai=True, project=None, location=None)
    assert model.client._api_client.api_key == expect_key


def test_proxy_env_does_not_bypass_transport(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("HTTPS_PROXY", "http://127.0.0.1:9")
    monkeypatch.setenv("ALL_PROXY", "http://127.0.0.1:9")
    fake = FakeVertex(ok(body(candidate("답"))))
    model, _, _ = fake_model(fake)
    assert asyncio.run(model.ainvoke("질문")).content == "답"
    assert len(fake.requests) == 1


def test_auth_error_surfaces_from_model_call_without_key() -> None:
    fake = FakeVertex(google_failure(401, "UNAUTHENTICATED", f"API key not valid: {KEY}", [
        {"@type": "type.googleapis.com/google.rpc.ErrorInfo", "reason": "API_KEY_INVALID"},
    ]))
    model, _, _ = fake_model(fake)
    with pytest.raises(ModelAuthError) as caught:
        asyncio.run(model.ainvoke("질문"))
    text = str(caught.value)
    assert "HTTP 401" in text and "API_KEY_INVALID" in text and "VERTEX_API_KEY" in text
    assert KEY not in text
    assert len(fake.requests) == 1


def test_sync_call_uses_same_transport() -> None:
    fake = FakeVertex(httpx.Response(503, json={"error": {"code": 503, "status": "UNAVAILABLE"}}), ok(body(candidate("답"))))
    settings = vertex_settings()
    model, _, nap = fake_model(fake, settings, use_async=False)
    assert model.invoke("질문").content == "답"
    assert len(fake.requests) == 2
    assert len(nap.waits) == 1
