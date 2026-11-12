import asyncio
import logging
from collections.abc import Sequence
from datetime import date
from itertools import zip_longest
from typing import Any

from semantic_agent.agent.issues import Intent, Issue
from semantic_agent.ledger import Ledger, compact
from semantic_agent.tools.client import SEARCH_FAIL, ToolClient

log = logging.getLogger(__name__)

SCOPE_K = 5
PICKS = 3
MAX_NAMES = 10
QUERY_CHARS = 200
HINT_HEAD = "먼저 읽을 조문:"


def _day(value: str) -> str | None:
    try:
        date.fromisoformat(value)
    except ValueError:
        return None
    return value


def scope_queries(original: str, intent: Intent) -> list[str]:
    keyword = " ".join([*intent.terms, intent.act]).strip()
    queries = [original.strip()[:QUERY_CHARS], keyword[:QUERY_CHARS]]
    return list(dict.fromkeys(q for q in queries if q))


def _label(hit: dict[str, Any]) -> str:
    return " ".join(str(hit.get(k) or "").strip() for k in ("law_name", "unit_key")).strip()


def read_labels(ledger: Ledger) -> set[str]:
    return {compact(a.label) for a in ledger.items.values() if a.kind == "law"}


class IntentScope:
    def __init__(self, client: ToolClient, k: int = SCOPE_K, picks: int = PICKS) -> None:
        self._client = client
        self._k = k
        self._picks = picks

    async def law_ids(self, names: Sequence[str]) -> list[str]:
        body = {"names": list(names)[:MAX_NAMES]}
        data, err = await self._client.fetch("POST", "/laws/scope", kind="search", fail=SEARCH_FAIL, json=body)
        if err or not isinstance(data, dict):
            log.info("intent scope names unresolved err=%s", bool(err))
            return []
        return [str(i) for i in data.get("law_ids") or [] if str(i).strip()]

    async def _hits(self, query: str, law_ids: list[str], as_of: str | None) -> list[dict[str, Any]]:
        body = {"query": query, "k": self._k, "as_of": as_of, "law_ids": law_ids}
        data, err = await self._client.fetch("POST", "/search/laws", kind="search", fail=SEARCH_FAIL, json=body)
        if err or not isinstance(data, dict):
            return []
        return [h for h in data.get("hits") or [] if isinstance(h, dict)]

    async def unread(self, original: str, issues: Sequence[Issue], ledger: Ledger) -> list[str]:
        seen = read_labels(ledger)
        picks: list[str] = []
        for issue in issues:
            intent = issue.intent
            if intent is None or not intent.laws:
                continue
            ids = await self.law_ids(intent.laws)
            if not ids:
                continue
            queries = scope_queries(original, intent)
            results = await asyncio.gather(*(self._hits(q, ids, _day(issue.as_of)) for q in queries))
            for hit in (h for row in zip_longest(*results) for h in row if h):
                label = _label(hit)
                key = compact(label)
                if not key or key in seen:
                    continue
                seen.add(key)
                picks.append(label)
                if len(picks) >= self._picks:
                    return picks
        return picks

    async def hint(self, original: str, issues: Sequence[Issue], ledger: Ledger) -> str:
        picks = await self.unread(original, issues, ledger)
        log.info("intent scope picks=%d", len(picks))
        return f"{HINT_HEAD} {', '.join(picks)}" if picks else ""
