import asyncio

import httpx
import pytest
from deepagents.profiles.harness.harness_profiles import _harness_profile_for_model
from langchain_core.tools import tool
from langchain_google_genai import ChatGoogleGenerativeAI
from langchain_openai import ChatOpenAI
from pydantic import SecretStr
from gemini_fake import (
    GENERATE_URL,
    KEY,
    MODEL_ID,
    STREAM_URL,
    FakeGemini,
    body,
    candidate,
    clean_google_env,
    fake_model,
    google_failure,
    ok,
    sse,
    gemini_settings,
)

from semantic_agent.agent.budget import BudgetExceeded
from semantic_agent.agent.build import _disable_general_purpose
from semantic_agent.agent.model import GEMINI_BASE_URL, GeminiChat, build_model
from semantic_agent.agent.model_errors import ModelAuthError, ModelConfigError, ModelOutputError
from semantic_agent.config import Settings

pytestmark = pytest.mark.usefixtures("clean_google_env")
__all__ = ["clean_google_env"]


def test_provider_defaults_to_openai_and_keeps_openai_model() -> None:
    settings = Settings(llm_base_url="http://127.0.0.1:1/v1", llm_api_key=SecretStr("x"), llm_model="m")
    assert settings.llm_provider == "openai"
    assert isinstance(build_model(settings), ChatOpenAI)


@pytest.mark.parametrize("provider", ["gemini", "vertex"])
def test_provider_and_values_read_from_env(monkeypatch: pytest.MonkeyPatch, provider: str) -> None:
    monkeypatch.setenv("LLM_PROVIDER", provider)
    monkeypatch.setenv("VERTEX_API_KEY", KEY)
    monkeypatch.setenv("VERTEX_MODEL", MODEL_ID)
    monkeypatch.setenv("VERTEX_MAX_CONCURRENCY", "3")
    monkeypatch.setenv("VERTEX_PROJECT", "p")
    settings = Settings(_env_file=None)
    assert settings.llm_provider == provider
    assert settings.vertex_api_key.get_secret_value() == KEY
    assert settings.vertex_model == MODEL_ID
    assert settings.vertex_max_concurrency == 3
    assert not hasattr(settings, "vertex_project") and not hasattr(settings, "vertex_location")
    assert settings.vertex_thinking_level is None and settings.vertex_thinking_budget is None


@pytest.mark.parametrize("provider", ["gemini", "vertex"])
def test_both_provider_names_build_gemini_api_model(provider: str) -> None:
    fake = FakeGemini(ok(body(candidate("답"))))
    model, _, _ = fake_model(fake, gemini_settings(llm_provider=provider))
    assert isinstance(model, GeminiChat)
    asyncio.run(model.ainvoke("질문"))
    assert str(fake.requests[0].url) == GENERATE_URL


def test_missing_names_follow_provider() -> None:
    base = {"internal_key": SecretStr("i"), "checkpoint_pg_url": SecretStr("p")}
    assert Settings(**base, llm_provider="vertex").missing() == ["vertex_api_key", "vertex_model"]
    assert Settings(**base, llm_provider="gemini").missing() == ["vertex_api_key", "vertex_model"]
    assert Settings(**base, llm_provider="vertex", vertex_api_key=SecretStr(KEY)).missing() == ["vertex_model"]
    assert Settings(**base, llm_provider="vertex", vertex_api_key=SecretStr(KEY), vertex_model=MODEL_ID).missing() == []
    assert Settings(**base, llm_provider="vertex", llm_base_url="u", llm_model="m").missing() == ["vertex_api_key", "vertex_model"]
    assert Settings(**base).missing() == ["llm_base_url", "llm_model"]
    assert Settings(**base, vertex_api_key=SecretStr(KEY), vertex_model=MODEL_ID).missing() == ["llm_base_url", "llm_model"]


def test_unknown_provider_and_bad_concurrency_are_rejected() -> None:
    with pytest.raises(ValueError):
        Settings(llm_provider="google")
    with pytest.raises(ValueError):
        Settings(vertex_max_concurrency=0)


def test_key_never_shows_in_settings_or_model_text() -> None:
    settings = gemini_settings()
    model, _, _ = fake_model(FakeGemini(ok(body(candidate()))), settings)
    for text in (repr(settings), str(settings), settings.model_dump_json(), repr(model), str(model)):
        assert KEY not in text


def test_request_goes_to_gemini_api_with_key_header_and_limits() -> None:
    fake = FakeGemini(ok(body(candidate("안녕"))))
    model, _, _ = fake_model(fake)
    assert isinstance(model, ChatGoogleGenerativeAI)
    reply = asyncio.run(model.ainvoke("질문"))
    request = fake.requests[0]
    assert str(request.url) == GENERATE_URL
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
    fake = FakeGemini(sse(body(candidate("가", finish=None), usage=(3, 1, 4)), body(candidate("나"), usage=(3, 2, 5))))
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
    fake = FakeGemini(sse(*chunks, body(candidate("끝"))))
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
    fake = FakeGemini(ok(body(call)))
    model, _, _ = fake_model(fake)
    reply = asyncio.run(model.bind_tools([find_law]).ainvoke("질문"))
    declared = [d["name"] for t in fake.json()["tools"] for d in t.get("functionDeclarations", [])]
    assert declared == ["find_law"]
    assert [(c["name"], c["args"]) for c in reply.tool_calls] == [("find_law", {"name": "행정절차법"})]


def test_provider_name_turns_off_general_purpose_subagent() -> None:
    model, _, _ = fake_model(FakeGemini(ok(body(candidate()))))
    assert model._get_ls_params()["ls_provider"] == "google_genai"
    _disable_general_purpose(model)
    assert _harness_profile_for_model(model, None).general_purpose_subagent.enabled is False


def test_thinking_is_sent_only_when_set() -> None:
    fake = FakeGemini(ok(body(candidate())))
    model, _, _ = fake_model(fake, gemini_settings(vertex_thinking_level="low"))
    asyncio.run(model.ainvoke("질문"))
    assert fake.json()["generationConfig"]["thinkingConfig"] == {"thinking_level": "LOW"}
    model, _, _ = fake_model(fake, gemini_settings(vertex_thinking_budget=256))
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
    fake = FakeGemini(ok(body(candidate("", finish=finish))), sse(body(candidate("일부", finish=finish))))
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
    fake = FakeGemini(ok(blocked), sse(blocked))
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
    model, _, _ = fake_model(FakeGemini(ok(body(candidate("답", finish=finish)))))
    assert asyncio.run(model.ainvoke("질문")).content == "답"


@pytest.mark.parametrize(
    "name",
    ["GOOGLE_API_KEY", "GEMINI_API_KEY", "GOOGLE_CLOUD_PROJECT", "GOOGLE_CLOUD_LOCATION", "GOOGLE_APPLICATION_CREDENTIALS"],
)
def test_ambient_google_env_is_refused(monkeypatch: pytest.MonkeyPatch, name: str) -> None:
    monkeypatch.setenv(name, "other-value")
    fake = FakeGemini(ok(body(candidate())))
    with pytest.raises(ModelConfigError) as caught:
        fake_model(fake)
    assert name in str(caught.value)
    assert "other-value" not in str(caught.value)
    assert fake.requests == []


@pytest.mark.parametrize(
    ("name", "value"),
    [
        ("GOOGLE_API_KEY", "other-value"),
        ("GEMINI_API_KEY", "other-value"),
        ("GOOGLE_CLOUD_PROJECT", "p"),
        ("GOOGLE_CLOUD_LOCATION", "us-central1"),
        ("GOOGLE_APPLICATION_CREDENTIALS", "C:/nowhere.json"),
        ("LANGSMITH_GATEWAY", "true"),
    ],
)
def test_library_keeps_our_key_and_address_in_gemini_mode(monkeypatch: pytest.MonkeyPatch, name: str, value: str) -> None:
    monkeypatch.setenv(name, value)
    fake = FakeGemini(ok(body(candidate())))
    model = GeminiChat(
        model=MODEL_ID,
        api_key=KEY,
        vertexai=False,
        base_url=GEMINI_BASE_URL,
        max_retries=1,
        client_args={"transport": httpx.MockTransport(fake.handler), "trust_env": False},
    )
    model.invoke("질문")
    assert not model.client._api_client.vertexai
    assert fake.requests[0].headers["x-goog-api-key"] == KEY
    assert str(fake.requests[0].url) == GENERATE_URL


def test_without_fixed_address_gateway_env_would_take_the_key(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("LANGSMITH_GATEWAY", "true")
    fake = FakeGemini(ok(body(candidate())))
    model = GeminiChat(model=MODEL_ID, api_key=KEY, vertexai=False, max_retries=1, client_args={"transport": httpx.MockTransport(fake.handler)})
    model.invoke("질문")
    assert fake.requests[0].url.host == "gateway.smith.langchain.com"


@pytest.mark.parametrize("name", ["GOOGLE_GENAI_USE_VERTEXAI", "GOOGLE_GENAI_USE_ENTERPRISE"])
def test_env_switching_to_vertex_is_refused_after_build(monkeypatch: pytest.MonkeyPatch, name: str) -> None:
    monkeypatch.setenv(name, "true")
    fake = FakeGemini(ok(body(candidate())))
    with pytest.raises(ModelConfigError) as caught:
        fake_model(fake)
    assert "Vertex AI 백엔드" in str(caught.value)
    assert KEY not in str(caught.value)
    assert fake.requests == []


def test_proxy_env_does_not_bypass_transport(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("HTTPS_PROXY", "http://127.0.0.1:9")
    monkeypatch.setenv("ALL_PROXY", "http://127.0.0.1:9")
    fake = FakeGemini(ok(body(candidate("답"))))
    model, _, _ = fake_model(fake)
    assert asyncio.run(model.ainvoke("질문")).content == "답"
    assert len(fake.requests) == 1


def test_auth_error_surfaces_from_model_call_without_key() -> None:
    fake = FakeGemini(google_failure(401, "UNAUTHENTICATED", f"API key not valid: {KEY}", [
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
    fake = FakeGemini(httpx.Response(503, json={"error": {"code": 503, "status": "UNAVAILABLE"}}), ok(body(candidate("답"))))
    settings = gemini_settings()
    model, _, nap = fake_model(fake, settings, use_async=False)
    assert model.invoke("질문").content == "답"
    assert len(fake.requests) == 2
    assert len(nap.waits) == 1


def test_gateway_env_does_not_move_our_requests(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("LANGSMITH_GATEWAY", "true")
    fake = FakeGemini(ok(body(candidate("답"))))
    model, _, _ = fake_model(fake)
    assert asyncio.run(model.ainvoke("질문")).content == "답"
    assert str(fake.requests[0].url) == GENERATE_URL
