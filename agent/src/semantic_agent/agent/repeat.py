import logging
import re
from typing import Any
from uuid import UUID

from langchain_core.callbacks import AsyncCallbackHandler

from semantic_agent.agent.budget import BudgetExceeded

log = logging.getLogger(__name__)

WINDOW = 256
BLANK_WINDOW = 1024
CHECK_EVERY = 64
TAIL = 2 * BLANK_WINDOW
LIMIT_REPEAT = "같은 말 되풀이"
_REP = re.compile(r"(.{1,48}?)\1{2,}", re.S)
_BLANK = re.compile(r"[\s─-╿]+")


def repeated(text: str) -> bool:
    for m in _REP.finditer(text):
        need = BLANK_WINDOW if _BLANK.fullmatch(m.group(1)) else WINDOW
        if m.end() - m.start() >= need:
            return True
    return False


class RepeatCut(AsyncCallbackHandler):
    raise_error = True

    def __init__(self) -> None:
        self._tail: dict[UUID, str] = {}
        self._fresh: dict[UUID, int] = {}

    async def on_llm_new_token(self, token: str, *, run_id: UUID, **kwargs: Any) -> None:
        if not token:
            return
        tail = (self._tail.get(run_id, "") + token)[-TAIL:]
        fresh = self._fresh.get(run_id, 0) + len(token)
        self._tail[run_id] = tail
        if fresh < CHECK_EVERY:
            self._fresh[run_id] = fresh
            return
        self._fresh[run_id] = 0
        if repeated(tail):
            self._forget(run_id)
            log.info("model output cut: repeated text")
            raise BudgetExceeded(LIMIT_REPEAT)

    async def on_llm_end(self, response: Any, *, run_id: UUID, **kwargs: Any) -> None:
        self._forget(run_id)

    async def on_llm_error(self, error: BaseException, *, run_id: UUID, **kwargs: Any) -> None:
        self._forget(run_id)

    def _forget(self, run_id: UUID) -> None:
        self._tail.pop(run_id, None)
        self._fresh.pop(run_id, None)
