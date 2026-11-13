from dataclasses import dataclass
from typing import Any

from semantic_agent.tools.client import REQUEST_ID, SEARCH_FAIL, THREAD_ID, ToolClient

CHECK_PATH = "/answer/check"
COVERAGE_PATH = "/research/coverage"


@dataclass(frozen=True)
class ReadCheck:
    client: ToolClient | None

    async def verdicts(self, sentences: list[str]) -> list[dict[str, Any]] | None:
        thread_id = THREAD_ID.get()
        if self.client is None or not thread_id or not sentences:
            return None
        body = {"thread_id": thread_id, "sentences": sentences}
        data, err = await self.client.fetch("POST", CHECK_PATH, kind="search", fail=SEARCH_FAIL, json=body)
        results = data.get("results") if isinstance(data, dict) and not err else None
        if not isinstance(results, list) or len(results) != len(sentences):
            return None
        return [r if isinstance(r, dict) else {} for r in results]

    async def read(self, sentences: list[str]) -> list[bool] | None:
        got = await self.verdicts(sentences)
        return None if got is None else [bool(r.get("ok")) for r in got]

    async def research_missing(self) -> list[str]:
        if self.client is None:
            return []
        params = {"request_id": REQUEST_ID.get()}
        data, err = await self.client.fetch("GET", COVERAGE_PATH, kind="search", fail=SEARCH_FAIL, params=params)
        missing = data.get("missing") if isinstance(data, dict) and not err else None
        return [str(m) for m in missing] if isinstance(missing, list) else []

    async def unread(self, sentences: list[str]) -> list[str]:
        got = await self.verdicts(sentences) or []
        out: list[str] = []
        for r in got:
            out.extend(str(u) for u in r.get("unread") or [] if str(u) not in out)
        return out
