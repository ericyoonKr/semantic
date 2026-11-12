import json
import random
import re
import time
from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from email.utils import parsedate_to_datetime
from enum import Enum
from typing import Any

import httpx

RETRY_ERRORS: tuple[type[Exception], ...] = (
    httpx.ConnectError,
    httpx.ConnectTimeout,
    httpx.ReadError,
    httpx.WriteError,
    httpx.RemoteProtocolError,
)
AUTH_STATUS = frozenset({401, 403})
LIMIT_STATUS = 429
SERVER_STATUS_MIN = 500
LENGTH_REASONS = frozenset({"MAX_TOKENS"})
SAFETY_REASONS = frozenset(
    {
        "SAFETY",
        "RECITATION",
        "BLOCKLIST",
        "PROHIBITED_CONTENT",
        "SPII",
        "IMAGE_SAFETY",
        "IMAGE_PROHIBITED_CONTENT",
        "IMAGE_RECITATION",
    }
)
RETRY_INFO = "type.googleapis.com/google.rpc.RetryInfo"
ERROR_INFO = "type.googleapis.com/google.rpc.ErrorInfo"
MESSAGE_LIMIT = 300
MASK = "***"
_DURATION = re.compile(r"^\s*(\d+(?:\.\d+)?)s\s*$")


class Verdict(Enum):
    PASS = "통과"
    RETRY = "재시도"
    AUTH = "인증 실패"


@dataclass(frozen=True)
class RetryPolicy:
    retries: int
    base_s: float = 1.0
    cap_s: float = 20.0
    max_wait_s: float = 60.0


@dataclass(frozen=True)
class GoogleError:
    status: str = ""
    reason: str = ""
    message: str = ""
    retry_delay_s: float | None = None


class ModelCallError(RuntimeError):
    pass


class ModelConfigError(ModelCallError):
    pass


class ModelAuthError(ModelCallError):
    def __init__(self, http_status: int, error: GoogleError, secrets: Iterable[str] = ()) -> None:
        self.http_status = http_status
        self.google_status = error.status
        self.reason = error.reason
        super().__init__(auth_message(http_status, error, secrets))


class ModelOutputError(ModelCallError):
    def __init__(self, kind: str, detail: str) -> None:
        self.kind = kind
        self.detail = detail
        super().__init__(f"모델 응답을 쓸 수 없습니다({kind}): {detail}")


def verdict(status: int) -> Verdict:
    if status in AUTH_STATUS:
        return Verdict.AUTH
    if status == LIMIT_STATUS or status >= SERVER_STATUS_MIN:
        return Verdict.RETRY
    return Verdict.PASS


def scrub(text: str, secrets: Iterable[str]) -> str:
    for secret in secrets:
        if secret:
            text = text.replace(secret, MASK)
    return text


def _error_body(data: Any) -> Mapping[str, Any]:
    if isinstance(data, list):
        data = next((item for item in data if isinstance(item, Mapping)), {})
    if not isinstance(data, Mapping):
        return {}
    error = data.get("error", data)
    return error if isinstance(error, Mapping) else {}


def duration_s(value: Any) -> float | None:
    if not isinstance(value, str):
        return None
    m = _DURATION.match(value)
    return float(m.group(1)) if m else None


def google_error(body: bytes) -> GoogleError:
    try:
        error = _error_body(json.loads(body.decode("utf-8", "replace")))
    except ValueError:
        return GoogleError(message=body.decode("utf-8", "replace"))
    reason = ""
    delay = None
    for detail in error.get("details") or []:
        if not isinstance(detail, Mapping):
            continue
        kind = detail.get("@type")
        if kind == ERROR_INFO and not reason:
            reason = str(detail.get("reason") or "")
        elif kind == RETRY_INFO and delay is None:
            delay = duration_s(detail.get("retryDelay"))
    return GoogleError(
        status=str(error.get("status") or ""),
        reason=reason,
        message=str(error.get("message") or ""),
        retry_delay_s=delay,
    )


def retry_after_s(headers: Mapping[str, str], now: float) -> float | None:
    value = (headers.get("retry-after") or "").strip()
    if not value:
        return None
    if value.isdigit():
        return float(value)
    try:
        when = parsedate_to_datetime(value)
    except (TypeError, ValueError):
        return None
    return max(0.0, when.timestamp() - now)


def server_wait_s(headers: Mapping[str, str], error: GoogleError, now: float | None = None) -> float | None:
    header = retry_after_s(headers, time.time() if now is None else now)
    return header if header is not None else error.retry_delay_s


def backoff_s(policy: RetryPolicy, attempt: int, rng: random.Random) -> float:
    step = min(policy.cap_s, policy.base_s * 2**attempt)
    return step / 2 + rng.uniform(0, step / 2)


def wait_s(policy: RetryPolicy, attempt: int, server: float | None, rng: random.Random) -> float | None:
    if attempt >= policy.retries:
        return None
    if server is None:
        return backoff_s(policy, attempt, rng)
    if server > policy.max_wait_s:
        return None
    return server + rng.uniform(0, policy.base_s)


def auth_message(http_status: int, error: GoogleError, secrets: Iterable[str] = ()) -> str:
    if http_status == 401:
        cause = "API 키가 틀렸거나 만료되었거나 지워졌습니다. VERTEX_API_KEY 값을 확인하십시오"
    else:
        cause = (
            "키에 이 요청을 할 권한이 없습니다. AI Studio에서 발급한 Gemini API 키인지, "
            "키의 API·IP 제한에 Generative Language API가 들어 있는지 확인하십시오"
        )
    code = ", ".join(part for part in (f"HTTP {http_status}", error.status, error.reason) if part)
    said = scrub(error.message, secrets)[:MESSAGE_LIMIT]
    tail = f" 서버 응답: {said}" if said else ""
    return f"Gemini API가 요청을 거부했습니다({code}). {cause}. 다시 시도하지 않습니다.{tail}"


def _blocked_categories(ratings: Any) -> list[str]:
    names = []
    for rating in ratings if isinstance(ratings, list) else []:
        if isinstance(rating, Mapping) and rating.get("blocked"):
            names.append(str(rating.get("category") or "?"))
    return names


def _block_reason(feedback: Any) -> str:
    if not isinstance(feedback, Mapping):
        return ""
    reason = feedback.get("block_reason")
    name = getattr(reason, "name", reason)
    if not name or name in (0, "BLOCKED_REASON_UNSPECIFIED"):
        return ""
    return str(name)


def finish_problem(
    finish_reason: str,
    block_reason: str,
    blocked: list[str],
    max_tokens: int | None,
) -> ModelOutputError | None:
    if block_reason:
        return ModelOutputError("입력 안전 차단", f"Gemini가 입력을 막았습니다(block_reason={block_reason})")
    if finish_reason in LENGTH_REASONS:
        cap = f"LLM_MAX_TOKENS={max_tokens}" if max_tokens else "최대 출력 토큰"
        return ModelOutputError(
            "출력 한도",
            f"출력이 {cap}에 닿아 중간에 끊겼습니다(finish_reason={finish_reason})",
        )
    if finish_reason in SAFETY_REASONS:
        where = f", 막힌 분류 {', '.join(blocked)}" if blocked else ""
        return ModelOutputError(
            "출력 안전 차단",
            f"Gemini가 답을 막았습니다(finish_reason={finish_reason}{where})",
        )
    return None


def result_problem(result: Any, max_tokens: int | None) -> ModelOutputError | None:
    outer = _block_reason((getattr(result, "llm_output", None) or {}).get("prompt_feedback"))
    for group in getattr(result, "generations", None) or []:
        for gen in group:
            info = getattr(gen, "generation_info", None) or {}
            meta = getattr(getattr(gen, "message", None), "response_metadata", None) or {}
            reason = str(info.get("finish_reason") or meta.get("finish_reason") or "")
            block = outer or _block_reason(meta.get("prompt_feedback"))
            ratings = info.get("safety_ratings") or meta.get("safety_ratings")
            problem = finish_problem(reason, block, _blocked_categories(ratings), max_tokens)
            if problem is not None:
                return problem
    if outer:
        return finish_problem("", outer, [], max_tokens)
    return None
