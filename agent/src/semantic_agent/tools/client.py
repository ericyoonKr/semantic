import logging
from contextvars import ContextVar
from datetime import date
from typing import Annotated, Any, Protocol

import httpx
from pydantic import AfterValidator, Field

from semantic_agent.config import Settings

log = logging.getLogger(__name__)

REQUEST_ID: ContextVar[str] = ContextVar("request_id", default="-")
SEARCH_FAIL = "검색 실패"
JUDGE_FAIL = "판정 불가"


def _real_date(value: str) -> str:
    date.fromisoformat(value)
    return value


IsoDate = Annotated[str, Field(pattern=r"^\d{4}-\d{2}-\d{2}$"), AfterValidator(_real_date)]
Limit = Annotated[int, Field(ge=1, le=20)]


def material(source: str, body: str) -> str:
    safe_source = source.replace('"', "'")
    safe_body = body.replace("</자료>", "&lt;/자료&gt;")
    return f'<자료 출처="{safe_source}">\n{safe_body}\n</자료>'


def unwrap_material(text: str) -> str:
    if not text.startswith("<자료 "):
        return text
    start = text.find(">") + 1
    end = text.rfind("</자료>")
    return text[start:end].strip("\n") if end > start else text


SHOWN_STATUSES = frozenset({400, 404, 409, 413, 422, 503})
REASON_CHARS = 200


def server_reason(response: httpx.Response) -> str | None:
    try:
        body = response.json()
    except ValueError:
        return None
    reason = body.get("error") if isinstance(body, dict) else None
    return str(reason)[:REASON_CHARS] if reason else None


class ToolClient(Protocol):
    async def fetch(
        self,
        method: str,
        path: str,
        *,
        kind: str,
        fail: str,
        json: Any = None,
        params: dict[str, Any] | None = None,
        content: Any = None,
        headers: dict[str, str] | None = None,
    ) -> tuple[Any, str | None]: ...

    async def open_bytes(self, path: str, *, kind: str) -> httpx.Response | None: ...

    async def aclose(self) -> None: ...


class RustClient:
    def __init__(self, settings: Settings, transport: httpx.AsyncBaseTransport | None = None) -> None:
        self._timeouts = settings.rust_timeout_s
        self._http = httpx.AsyncClient(
            base_url=settings.rust_api_url,
            headers={"x-internal-key": settings.internal_key.get_secret_value()},
            transport=transport,
        )

    async def fetch(
        self,
        method: str,
        path: str,
        *,
        kind: str,
        fail: str,
        json: Any = None,
        params: dict[str, Any] | None = None,
        content: Any = None,
        headers: dict[str, str] | None = None,
    ) -> tuple[Any, str | None]:
        request_id = REQUEST_ID.get()
        sent_headers = {"x-request-id": request_id, **(headers or {})}
        clean_params = {k: v for k, v in (params or {}).items() if v is not None}
        try:
            response = await self._http.request(
                method,
                path,
                json=json,
                params=clean_params,
                content=content,
                headers=sent_headers,
                timeout=self._timeouts[kind],
            )
            response.raise_for_status()
        except httpx.TimeoutException:
            log.warning("tool call timed out request_id=%s path_kind=%s", request_id, kind)
            return None, f"{fail}: 시간 초과 (요청 ID {request_id})"
        except httpx.HTTPStatusError as exc:
            status = exc.response.status_code
            log.warning("tool call status=%s request_id=%s", status, request_id)
            reason = server_reason(exc.response) if status in SHOWN_STATUSES else None
            said = reason if reason and reason.startswith(fail) else f"{fail}: {reason or '지금 처리할 수 없음'}"
            return None, f"{said} (요청 ID {request_id})"
        except httpx.HTTPError as exc:
            log.warning("tool call failed type=%s request_id=%s", type(exc).__name__, request_id)
            return None, f"{fail}: 지금 연결되지 않음 (요청 ID {request_id})"
        if response.status_code == 204 or not response.content:
            return {}, None
        try:
            return response.json(), None
        except ValueError:
            return None, f"{fail}: 응답 형식 오류 (요청 ID {request_id})"

    async def open_bytes(self, path: str, *, kind: str) -> httpx.Response | None:
        request = self._http.build_request(
            "GET", path, headers={"x-request-id": REQUEST_ID.get()}, timeout=self._timeouts[kind]
        )
        try:
            return await self._http.send(request, stream=True)
        except httpx.HTTPError as exc:
            log.warning("byte stream failed type=%s request_id=%s", type(exc).__name__, REQUEST_ID.get())
            return None

    async def aclose(self) -> None:
        await self._http.aclose()
