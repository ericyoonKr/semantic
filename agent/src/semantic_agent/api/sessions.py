import asyncio
import contextlib
import time
from collections.abc import Coroutine
from dataclasses import dataclass, field
from typing import Any

from semantic_agent.api.runlog import RunLog
from semantic_agent.ledger import Ledger


@dataclass
class SessionSlot:
    lock: asyncio.Lock = field(default_factory=asyncio.Lock)
    running_ask: asyncio.Task | None = None
    log: RunLog | None = None
    ledger: Ledger = field(default_factory=Ledger)
    ledger_loaded: bool = False
    last_used: float = field(default_factory=time.monotonic)


class SessionRegistry:
    def __init__(self) -> None:
        self._slots: dict[str, SessionSlot] = {}

    def get(self, session_id: str) -> SessionSlot:
        slot = self._slots.get(session_id)
        if slot is None:
            slot = SessionSlot()
            self._slots[session_id] = slot
        slot.last_used = time.monotonic()
        return slot

    def cancel_all(self) -> None:
        for slot in self._slots.values():
            if slot.running_ask is not None and not slot.running_ask.done():
                slot.running_ask.cancel()

    def live(self, session_id: str) -> RunLog | None:
        slot = self._slots.get(session_id)
        log = slot.log if slot is not None else None
        return log if log is not None and not log.done else None

    def drop(self, session_id: str) -> None:
        self._slots.pop(session_id, None)

    def prune(self, idle_s: float) -> int:
        now = time.monotonic()
        stale = [
            sid
            for sid, slot in self._slots.items()
            if not slot.lock.locked() and slot.running_ask is None and now - slot.last_used > idle_s
        ]
        for sid in stale:
            del self._slots[sid]
        return len(stale)


async def run_ask(slot: SessionSlot, coro: Coroutine[Any, Any, Any]) -> Any:
    try:
        await slot.lock.acquire()
    except BaseException:
        coro.close()
        raise
    try:
        task = asyncio.ensure_future(coro)
        slot.running_ask = task
        return await task
    finally:
        slot.running_ask = None
        slot.last_used = time.monotonic()
        slot.lock.release()


async def run_upload(slot: SessionSlot, coro: Coroutine[Any, Any, Any]) -> Any:
    running = slot.running_ask
    if running is not None and not running.done():
        running.cancel()
        with contextlib.suppress(asyncio.CancelledError, Exception):
            await running
    async with slot.lock:
        try:
            return await coro
        finally:
            slot.last_used = time.monotonic()
