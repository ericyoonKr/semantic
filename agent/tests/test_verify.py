import asyncio
from typing import Any

from semantic_agent.api.sources import build_sources
from semantic_agent.api.verify import CHANGED, NO_ID, NOT_FOUND, verify_sources
from semantic_agent.ledger import Ledger

BODY = "① 국가나 지방자치단체는 손해를 배상하여야 한다."
LAW = {"law_name": "국가배상법", "unit_key": "제2조", "enforcement_date": "2025-01-07", "body": BODY, "law_id": "001", "version": {"version_id": "v1"}}


class Stub:
    def __init__(self, laws: dict[str, Any], cases: dict[str, Any]) -> None:
        self.laws, self.cases, self.calls = laws, cases, []

    async def fetch(self, method: str, path: str, **kw: Any) -> tuple[Any, str | None]:
        self.calls.append(path)
        if path == "/laws/unit":
            found = self.laws.get(kw["params"]["law"])
            return (found, None) if found else (None, "없음")
        found = self.cases.get(path.rsplit("/", 1)[-1])
        return (found, None) if found else (None, "없음")

    async def aclose(self) -> None:
        return None


def _ledger() -> Ledger:
    ledger = Ledger()
    ledger.add_law(LAW)
    ledger.add_case({"court": "대법원", "case_no": "2020두1", "decided_on": "2021-01-01", "sections": [{"body": "본문"}]}, "77")
    return ledger


def _items(ledger: Ledger) -> list[dict[str, Any]]:
    return build_sources(ledger, [(1, None, "국가가 손해를 배상"), (2, None, "본문")])


def test_sources_that_match_db_ids_and_text_are_verified() -> None:
    ledger = _ledger()
    stub = Stub({"001": {**LAW}}, {"77": {"case_no": "2020두1"}})
    out = asyncio.run(verify_sources(stub, ledger, _items(ledger)))
    assert [(s["id"], s["verified"]) for s in out] == [("v1", True), ("77", True)]


def test_missing_changed_or_unidentified_sources_are_flagged() -> None:
    ledger = _ledger()
    changed = {**LAW, "body": "다른 글"}
    out = asyncio.run(verify_sources(Stub({"001": changed}, {}), ledger, _items(ledger)))
    assert [(s["verified"], s["verify_note"]) for s in out] == [(False, CHANGED), (False, NOT_FOUND)]
    other = {**LAW, "version": {"version_id": "v2"}}
    assert asyncio.run(verify_sources(Stub({"001": other}, {}), ledger, _items(ledger)))[0]["verify_note"] == NOT_FOUND
    assert all(s["verify_note"] == NO_ID for s in asyncio.run(verify_sources(None, ledger, _items(ledger))))
