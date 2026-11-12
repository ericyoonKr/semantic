import asyncio
import logging
import random
import threading
import time
from collections.abc import AsyncIterator, Awaitable, Callable, Iterator
from typing import Any
from weakref import WeakKeyDictionary

import httpx

from semantic_agent.agent.model_errors import (
    RETRY_ERRORS,
    ModelAuthError,
    RetryPolicy,
    Verdict,
    google_error,
    scrub,
    server_wait_s,
    verdict,
    wait_s,
)

log = logging.getLogger(__name__)

KEY_HEADER = "x-goog-api-key"


class _Once:
    def __init__(self, release: Callable[[], None]) -> None:
        self._release = release
        self._done = False
        self._lock = threading.Lock()

    def __call__(self) -> None:
        with self._lock:
            if self._done:
                return
            self._done = True
        self._release()


class _AsyncReleasing(httpx.AsyncByteStream):
    def __init__(self, inner: Any, release: _Once, owner: Any, forget: Callable[["_AsyncReleasing"], None]) -> None:
        self._inner = inner
        self._release = release
        self._forget = forget
        self.owner = owner
        self.closed = False

    async def __aiter__(self) -> AsyncIterator[bytes]:
        async for part in self._inner:
            yield part

    async def aclose(self) -> None:
        if self.closed:
            return
        self.closed = True
        try:
            await self._inner.aclose()
        finally:
            self._forget(self)
            self._release()


class _SyncReleasing(httpx.SyncByteStream):
    def __init__(self, inner: Any, release: _Once) -> None:
        self._inner = inner
        self._release = release

    def __iter__(self) -> Iterator[bytes]:
        yield from self._inner

    def close(self) -> None:
        try:
            self._inner.close()
        finally:
            self._release()


class ModelTransport(httpx.AsyncBaseTransport, httpx.BaseTransport):
    def __init__(
        self,
        policy: RetryPolicy,
        timeout: httpx.Timeout,
        limit: int,
        *,
        inner: Any = None,
        asleep: Callable[[float], Awaitable[Any]] = asyncio.sleep,
        sleep: Callable[[float], Any] = time.sleep,
        rng: random.Random | None = None,
    ) -> None:
        if limit < 1:
            raise ValueError("동시 호출 상한은 1 이상이어야 합니다")
        self.policy = policy
        self.timeout = timeout
        self.limit = limit
        self._async_inner = inner if inner is not None else httpx.AsyncHTTPTransport()
        self._sync_inner = inner if inner is not None else httpx.HTTPTransport()
        self._asleep = asleep
        self._sleep = sleep
        self._rng = rng or random.Random()
        self._async_gates: WeakKeyDictionary[asyncio.AbstractEventLoop, asyncio.Semaphore] = WeakKeyDictionary()
        self._sync_gate = threading.BoundedSemaphore(limit)
        self._open: dict[Any, set[_AsyncReleasing]] = {}

    def _async_gate(self) -> asyncio.Semaphore:
        loop = asyncio.get_running_loop()
        gate = self._async_gates.get(loop)
        if gate is None:
            gate = asyncio.Semaphore(self.limit)
            self._async_gates[loop] = gate
        return gate

    def _prepare(self, request: httpx.Request) -> None:
        request.extensions["timeout"] = self.timeout.as_dict()

    def _secrets(self, request: httpx.Request) -> tuple[str, ...]:
        key = request.headers.get(KEY_HEADER)
        return (key,) if key else ()

    def _where(self, request: httpx.Request) -> str:
        return f"{request.method} {request.url.host}{request.url.path}"

    def _status_step(self, request: httpx.Request, response: httpx.Response, body: bytes, attempt: int) -> float | None:
        error = google_error(body)
        where = self._where(request)
        if verdict(response.status_code) is Verdict.AUTH:
            log.error(
                "vertex refused status=%s google_status=%s reason=%s where=%s attempt=%d",
                response.status_code,
                error.status,
                error.reason,
                where,
                attempt + 1,
            )
            raise ModelAuthError(response.status_code, error, self._secrets(request))
        server = server_wait_s(response.headers, error)
        wait = wait_s(self.policy, attempt, server, self._rng)
        if wait is None:
            log.warning(
                "vertex gave up status=%s google_status=%s where=%s attempts=%d server_wait_s=%s max_wait_s=%s",
                response.status_code,
                error.status,
                where,
                attempt + 1,
                server,
                self.policy.max_wait_s,
            )
        else:
            log.warning(
                "vertex retry status=%s google_status=%s where=%s attempt=%d wait_s=%.2f server_wait_s=%s",
                response.status_code,
                error.status,
                where,
                attempt + 1,
                wait,
                server,
            )
        return wait

    def _error_step(self, request: httpx.Request, exc: Exception, attempt: int) -> float | None:
        wait = wait_s(self.policy, attempt, None, self._rng)
        detail = scrub(str(exc), self._secrets(request))
        if wait is None:
            log.warning(
                "vertex gave up error=%s detail=%s where=%s attempts=%d",
                type(exc).__name__,
                detail,
                self._where(request),
                attempt + 1,
            )
        else:
            log.warning(
                "vertex retry error=%s detail=%s where=%s attempt=%d wait_s=%.2f",
                type(exc).__name__,
                detail,
                self._where(request),
                attempt + 1,
                wait,
            )
        return wait

    async def handle_async_request(self, request: httpx.Request) -> httpx.Response:
        self._prepare(request)
        gate = self._async_gate()
        if gate.locked():
            log.info("vertex waiting for slot limit=%d where=%s", self.limit, self._where(request))
        await gate.acquire()
        release = _Once(gate.release)
        try:
            response = await self._async_attempts(request)
        except BaseException:
            release()
            raise
        if verdict(response.status_code) is not Verdict.PASS or response.is_closed:
            release()
            return response
        owner = asyncio.current_task()
        stream = _AsyncReleasing(response.stream, release, owner, self._forget)
        self._open.setdefault(owner, set()).add(stream)
        response.stream = stream
        return response

    def _forget(self, stream: _AsyncReleasing) -> None:
        streams = self._open.get(stream.owner)
        if streams is None:
            return
        streams.discard(stream)
        if not streams:
            self._open.pop(stream.owner, None)

    def open_streams(self, owner: Any) -> int:
        return len(self._open.get(owner, ()))

    async def close_left(self, owner: Any) -> int:
        streams = list(self._open.get(owner, ()))
        for stream in streams:
            log.warning("vertex stream closed after caller stopped reading limit=%d", self.limit)
            await stream.aclose()
        return len(streams)

    async def _async_attempts(self, request: httpx.Request) -> httpx.Response:
        attempt = 0
        while True:
            try:
                response = await self._async_inner.handle_async_request(request)
            except RETRY_ERRORS as exc:
                wait = self._error_step(request, exc, attempt)
                if wait is None:
                    raise
                await self._asleep(wait)
                attempt += 1
                continue
            if verdict(response.status_code) is Verdict.PASS:
                return response
            try:
                body = await response.aread()
            finally:
                await response.aclose()
            wait = self._status_step(request, response, body, attempt)
            if wait is None:
                return response
            await self._asleep(wait)
            attempt += 1

    def handle_request(self, request: httpx.Request) -> httpx.Response:
        self._prepare(request)
        self._sync_gate.acquire()
        release = _Once(self._sync_gate.release)
        try:
            response = self._sync_attempts(request)
        except BaseException:
            release()
            raise
        if verdict(response.status_code) is not Verdict.PASS or response.is_closed:
            release()
            return response
        response.stream = _SyncReleasing(response.stream, release)
        return response

    def _sync_attempts(self, request: httpx.Request) -> httpx.Response:
        attempt = 0
        while True:
            try:
                response = self._sync_inner.handle_request(request)
            except RETRY_ERRORS as exc:
                wait = self._error_step(request, exc, attempt)
                if wait is None:
                    raise
                self._sleep(wait)
                attempt += 1
                continue
            if verdict(response.status_code) is Verdict.PASS:
                return response
            try:
                body = response.read()
            finally:
                response.close()
            wait = self._status_step(request, response, body, attempt)
            if wait is None:
                return response
            self._sleep(wait)
            attempt += 1

    async def aclose(self) -> None:
        await self._async_inner.aclose()

    def close(self) -> None:
        self._sync_inner.close()
