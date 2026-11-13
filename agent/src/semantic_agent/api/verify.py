import asyncio
from typing import Any

from semantic_agent.ledger import Authority, Ledger, compact
from semantic_agent.tools.client import ToolClient

VERIFY_FAIL = "근거 대조 실패"
NOT_FOUND = "DB에서 찾지 못함"
CHANGED = "DB 원문과 다름"
NO_ID = "ID 없음"


async def _check_annex_list(client: ToolClient, item: Authority) -> str | None:
    from semantic_agent.tools.annex import format_annex_list

    data, err = await client.fetch("GET", "/laws/annexes", kind="search", fail=VERIFY_FAIL, params={"law": item.law_id})
    if err or not isinstance(data, dict):
        return NOT_FOUND
    return None if compact(format_annex_list(data)) == compact(item.body) else CHANGED


async def _check_law(client: ToolClient, item: Authority) -> str | None:
    if not item.law_id or not item.source_id:
        return NO_ID
    if item.unit_key == "별표 목록":
        return await _check_annex_list(client, item)
    params = {"law": item.law_id, "unit": item.unit_key, "as_of": item.enforced or None}
    data, err = await client.fetch("GET", "/laws/unit", kind="search", fail=VERIFY_FAIL, params=params)
    if err or not isinstance(data, dict):
        return NOT_FOUND
    if str((data.get("version") or {}).get("version_id") or "") != item.source_id:
        return NOT_FOUND
    if compact(str(data.get("body") or "")) != compact(item.body):
        return CHANGED
    return None


async def _check_case(client: ToolClient, item: Authority) -> str | None:
    if not item.source_id:
        return NO_ID
    data, err = await client.fetch("GET", f"/cases/{item.source_id}", kind="search", fail=VERIFY_FAIL)
    if err or not isinstance(data, dict):
        return NOT_FOUND
    if compact(str(data.get("case_no") or "")) != item.case_no:
        return CHANGED
    return None


async def check(client: ToolClient, item: Authority) -> str | None:
    try:
        return await (_check_law(client, item) if item.kind == "law" else _check_case(client, item))
    except Exception:
        return NOT_FOUND


async def verify_sources(client: ToolClient | None, ledger: Ledger, items: list[dict[str, Any]]) -> list[dict[str, Any]]:
    if client is None:
        return [{**s, "verified": False, "verify_note": NO_ID} for s in items]
    found = [ledger.get(int(s["ref"])) for s in items]
    notes = await asyncio.gather(*(check(client, a) if a is not None else asyncio.sleep(0, NOT_FOUND) for a in found))
    return [{**s, "verified": note is None, "verify_note": note or ""} for s, note in zip(items, notes, strict=True)]
