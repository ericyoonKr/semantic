import re
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, replace
from typing import Any

from semantic_agent.agent.issues import Issue, check_refs
from semantic_agent.ledger import Authority, Ledger

REF = re.compile(r"^\s*\[?근거\s*(\d+)\]?")


@dataclass(frozen=True)
class Finding:
    claim: str
    source: str
    quote: str
    stance: str = "neutral"
    annex_used: bool = False
    old_law_case: bool = False


@dataclass(frozen=True)
class Findings:
    found: list[Finding]
    not_found: list[str]
    dropped: int
    limit: str | None
    reverse_query: str | None = None
    issues: tuple[Issue, ...] = ()
    irrelevant: int = 0


Judge = Callable[[str, list[dict[str, Any]]], Awaitable[list[int] | None]]
JUDGE_TEXT_CHARS = 800


def _ref_of(source: str) -> int | None:
    match = REF.match(source)
    return int(match.group(1)) if match else None


async def judge_relevance(findings: Findings, ledger: Ledger, judge: Judge, issue: str) -> Findings:
    items: list[dict[str, Any]] = []
    for item in findings.found:
        ref = _ref_of(item.source)
        authority = ledger.get(ref) if ref is not None else None
        if authority is None or any(i["ref"] == ref for i in items):
            continue
        items.append({"ref": ref, "label": authority.display(), "text": authority.body[:JUDGE_TEXT_CHARS]})
    keep = await judge(issue, items) if items else None
    if keep is None:
        return findings
    kept = [item for item in findings.found if _ref_of(item.source) in keep]
    return replace(findings, found=kept, irrelevant=len(findings.found) - len(kept))


def _keep(findings: Findings, judge: Callable[[Finding], Finding | None]) -> Findings:
    kept: list[Finding] = []
    dropped = findings.dropped
    for item in findings.found:
        checked = judge(item)
        if checked is None:
            dropped += 1
        else:
            kept.append(checked)
    return replace(findings, found=kept, dropped=dropped)


def _authority(ledger: Ledger, source: str) -> Authority | None:
    match = REF.match(source)
    return ledger.get(int(match.group(1))) if match else None


def _shown(authority: Authority) -> str:
    return f"[근거 {authority.ref}] {authority.display()}"


def check_research(findings: Findings, ledger: Ledger) -> Findings:
    def judge(item: Finding) -> Finding | None:
        authority = _authority(ledger, item.source)
        if authority is None or not authority.has_quote(item.quote):
            return None
        return replace(item, source=_shown(authority))

    def ref(source: str) -> str | None:
        authority = _authority(ledger, source)
        return _shown(authority) if authority else None

    return replace(_keep(findings, judge), issues=check_refs(findings.issues, ref))
