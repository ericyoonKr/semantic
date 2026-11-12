import asyncio
import json
import random
from datetime import UTC, datetime
from email.utils import format_datetime

import httpx
import pytest
from vertex_fake import KEY, FakeVertex, body, candidate, clean_google_env, fake_model, google_failure, ok, sse, vertex_settings

from semantic_agent.agent.budget import BudgetExceeded
from semantic_agent.agent.model_errors import (
    GoogleError,
    ModelAuthError,
    RetryPolicy,
    Verdict,
    backoff_s,
    google_error,
    retry_after_s,
    server_wait_s,
    verdict,
    wait_s,
)

pytestmark = pytest.mark.usefixtures("clean_google_env")
__all__ = ["clean_google_env"]

RETRY_INFO = "type.googleapis.com/google.rpc.RetryInfo"


@pytest.mark.parametrize(
    ("status", "expected"),
    [
        (200, Verdict.PASS),
        (400, Verdict.PASS),
        (404, Verdict.PASS),
        (401, Verdict.AUTH),
        (403, Verdict.AUTH),
        (429, Verdict.RETRY),
        (500, Verdict.RETRY),
        (502, Verdict.RETRY),
        (503, Verdict.RETRY),
        (504, Verdict.RETRY),
    ],
)
def test_status_verdict(status: int, expected: Verdict) -> None:
    assert verdict(status) is expected


def test_retry_after_header_seconds_and_date() -> None:
    now = 1_000_000.0
    assert retry_after_s({"retry-after": "7"}, now) == 7.0
    when = format_datetime(datetime.fromtimestamp(now + 12, UTC), usegmt=True)
    assert retry_after_s({"retry-after": when}, now) == pytest.approx(12.0)
    assert retry_after_s({"retry-after": "언젠가"}, now) is None
    assert retry_after_s({}, now) is None


def test_google_error_reads_status_reason_and_retry_delay() -> None:
    raw = json.dumps(
        {
            "error": {
                "code": 429,
                "status": "RESOURCE_EXHAUSTED",
                "message": "quota",
                "details": [
                    {"@type": "type.googleapis.com/google.rpc.ErrorInfo", "reason": "RATE_LIMIT_EXCEEDED"},
                    {"@type": RETRY_INFO, "retryDelay": "2.5s"},
                ],
            }
        }
    ).encode()
    error = google_error(raw)
    assert (error.status, error.reason, error.retry_delay_s) == ("RESOURCE_EXHAUSTED", "RATE_LIMIT_EXCEEDED", 2.5)
    listed = google_error(b'[{"error": {"code": 503, "status": "UNAVAILABLE"}}]')
    assert listed.status == "UNAVAILABLE"
    assert google_error(b"<html>bad gateway</html>").message == "<html>bad gateway</html>"


def test_header_wins_over_body_delay() -> None:
    error = GoogleError(retry_delay_s=30.0)
    assert server_wait_s({"retry-after": "3"}, error, now=0.0) == 3.0
    assert server_wait_s({}, error, now=0.0) == 30.0


def test_wait_rules() -> None:
    policy = RetryPolicy(retries=3, base_s=1.0, cap_s=20.0, max_wait_s=60.0)
    rng = random.Random(7)
    for attempt, step in [(0, 1.0), (1, 2.0), (2, 4.0), (6, 20.0)]:
        for _ in range(50):
            assert step / 2 <= backoff_s(policy, attempt, rng) <= step
    assert wait_s(policy, 3, None, rng) is None
    assert wait_s(policy, 0, 61.0, rng) is None
    for _ in range(50):
        assert 5.0 <= wait_s(policy, 0, 5.0, rng) <= 6.0


def test_auth_message_hides_key() -> None:
    error = GoogleError(status="PERMISSION_DENIED", reason="API_KEY_SERVICE_BLOCKED", message=f"key {KEY} blocked")
    text = str(ModelAuthError(403, error, [KEY]))
    assert KEY not in text
    assert "HTTP 403" in text and "PERMISSION_DENIED" in text and "API_KEY_SERVICE_BLOCKED" in text
    assert "다시 시도하지 않습니다" in text


def test_429_waits_for_retry_after_then_succeeds() -> None:
    fake = FakeVertex(
        google_failure(429, "RESOURCE_EXHAUSTED", "quota", **{"Retry-After": "3"}),
        ok(body(candidate("답"))),
    )
    model, _, nap = fake_model(fake)
    assert asyncio.run(model.ainvoke("질문")).content == "답"
    assert len(fake.requests) == 2
    assert len(nap.waits) == 1 and 3.0 <= nap.waits[0] <= 4.0


def test_429_uses_body_retry_delay_when_no_header() -> None:
    fake = FakeVertex(
        google_failure(429, "RESOURCE_EXHAUSTED", "quota", [{"@type": RETRY_INFO, "retryDelay": "9s"}]),
        ok(body(candidate("답"))),
    )
    model, _, nap = fake_model(fake)
    asyncio.run(model.ainvoke("질문"))
    assert 9.0 <= nap.waits[0] <= 10.0


def test_429_too_long_wait_gives_up_at_once() -> None:
    fake = FakeVertex(google_failure(429, "RESOURCE_EXHAUSTED", "quota", **{"Retry-After": "3600"}), ok(body(candidate())))
    model, _, nap = fake_model(fake)
    with pytest.raises(Exception) as caught:
        asyncio.run(model.ainvoke("질문"))
    assert "429" in str(caught.value)
    assert len(fake.requests) == 1 and nap.waits == []


@pytest.mark.parametrize("retries", [0, 1, 3])
def test_5xx_retries_exactly_configured_times(retries: int) -> None:
    fake = FakeVertex(httpx.Response(503, json={"error": {"code": 503, "status": "UNAVAILABLE"}}))
    model, _, nap = fake_model(fake, vertex_settings(llm_max_retries=retries))
    with pytest.raises(Exception) as caught:
        asyncio.run(model.ainvoke("질문"))
    assert "503" in str(caught.value)
    assert len(fake.requests) == retries + 1
    assert len(nap.waits) == retries


@pytest.mark.parametrize(
    "error",
    [httpx.ConnectError("refused"), httpx.RemoteProtocolError("server disconnected"), httpx.ReadError("reset"), httpx.ConnectTimeout("slow")],
)
def test_connection_drop_is_retried(error: Exception) -> None:
    fake = FakeVertex(error, ok(body(candidate("답"))))
    model, _, nap = fake_model(fake)
    assert asyncio.run(model.ainvoke("질문")).content == "답"
    assert len(fake.requests) == 2 and len(nap.waits) == 1


def test_connection_drop_gives_up_after_retries() -> None:
    fake = FakeVertex(httpx.ConnectError("refused"))
    model, _, _ = fake_model(fake, vertex_settings(llm_max_retries=2))
    with pytest.raises(httpx.ConnectError):
        asyncio.run(model.ainvoke("질문"))
    assert len(fake.requests) == 3


def test_read_timeout_is_not_retried() -> None:
    fake = FakeVertex(httpx.ReadTimeout("slow"), ok(body(candidate())))
    model, _, _ = fake_model(fake)
    with pytest.raises(httpx.ReadTimeout):
        asyncio.run(model.ainvoke("질문"))
    assert len(fake.requests) == 1


@pytest.mark.parametrize(("status", "google_status"), [(401, "UNAUTHENTICATED"), (403, "PERMISSION_DENIED")])
def test_auth_failure_is_not_retried(status: int, google_status: str) -> None:
    fake = FakeVertex(google_failure(status, google_status, f"bad key {KEY}"), ok(body(candidate())))
    model, _, nap = fake_model(fake, vertex_settings(llm_max_retries=3))
    with pytest.raises(ModelAuthError) as caught:
        asyncio.run(model.ainvoke("질문"))
    assert caught.value.http_status == status and caught.value.google_status == google_status
    assert KEY not in str(caught.value)
    assert len(fake.requests) == 1 and nap.waits == []


def test_400_is_left_to_library_without_retry() -> None:
    fake = FakeVertex(google_failure(400, "INVALID_ARGUMENT", "bad request"), ok(body(candidate())))
    model, _, _ = fake_model(fake, vertex_settings(llm_max_retries=3))
    with pytest.raises(Exception) as caught:
        asyncio.run(model.ainvoke("질문"))
    assert not isinstance(caught.value, ModelAuthError)
    assert len(fake.requests) == 1


@pytest.mark.parametrize("read_already", [False, True])
def test_concurrent_calls_stay_under_limit(read_already: bool) -> None:
    state = {"now": 0, "peak": 0}

    async def slow(request: httpx.Request) -> httpx.Response:
        state["now"] += 1
        state["peak"] = max(state["peak"], state["now"])
        await asyncio.sleep(0.02)
        state["now"] -= 1
        return httpx.Response(200, json=body(candidate("답"))) if read_already else ok(body(candidate("답")))

    fake = FakeVertex(slow)
    model, _, _ = fake_model(fake, limit=2)

    async def run() -> list:
        return await asyncio.gather(*(model.ainvoke(f"질문 {i}") for i in range(6)))

    replies = asyncio.run(asyncio.wait_for(run(), timeout=5.0))
    assert [r.content for r in replies] == ["답"] * 6
    assert state["peak"] == 2
    assert len(fake.requests) == 6


def test_abandoned_stream_gives_slot_back() -> None:
    chunks = [body(candidate(f"조각{i} ", finish=None)) for i in range(5)]
    fake = FakeVertex(sse(*chunks, body(candidate("끝"))), ok(body(candidate("다음 답"))))
    model, _, _ = fake_model(fake, limit=1)

    async def run() -> str:
        async for _ in model.astream("질문"):
            break
        reply = await asyncio.wait_for(model.ainvoke("다음"), timeout=2.0)
        return reply.text

    assert asyncio.run(run()) == "다음 답"


def test_failed_attempts_give_slot_back() -> None:
    fake = FakeVertex(google_failure(401, "UNAUTHENTICATED"), ok(body(candidate("답"))))
    model, _, _ = fake_model(fake, limit=1)

    async def run() -> str:
        with pytest.raises(ModelAuthError):
            await model.ainvoke("질문")
        return (await asyncio.wait_for(model.ainvoke("다음"), timeout=2.0)).text

    assert asyncio.run(run()) == "답"


def test_repeat_cut_abort_gives_slot_and_connection_back() -> None:
    chunks = [body(candidate("반복되는 문장입니다. " * 4, finish=None)) for _ in range(30)]
    fake = FakeVertex(sse(*chunks, body(candidate("끝"))), ok(body(candidate("다음 답"))))
    model, transport, _ = fake_model(fake, limit=1)

    async def run() -> tuple[str, int]:
        with pytest.raises(BudgetExceeded):
            async for _ in model.astream("질문"):
                pass
        reply = await asyncio.wait_for(model.ainvoke("다음"), timeout=2.0)
        return reply.text, transport.open_streams(asyncio.current_task())

    assert asyncio.run(run()) == ("다음 답", 0)
