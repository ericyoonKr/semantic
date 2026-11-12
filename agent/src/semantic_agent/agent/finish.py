import logging
from typing import Any
from uuid import UUID

from langchain_core.callbacks import AsyncCallbackHandler
from langchain_core.outputs import LLMResult

from semantic_agent.agent.model_errors import result_problem

log = logging.getLogger(__name__)


class FinishCheck(AsyncCallbackHandler):
    raise_error = True

    def __init__(self, max_tokens: int | None) -> None:
        self.max_tokens = max_tokens

    async def on_llm_end(self, response: LLMResult, *, run_id: UUID, **kwargs: Any) -> None:
        problem = result_problem(response, self.max_tokens)
        if problem is None:
            return
        log.error("model output rejected kind=%s detail=%s run_id=%s", problem.kind, problem.detail, run_id)
        raise problem
