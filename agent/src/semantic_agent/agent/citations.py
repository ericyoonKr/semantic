import logging
import re
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, replace
from typing import Any

from semantic_agent.agent.issues import Issue, check_refs
from semantic_agent.ledger import Authority, Ledger, compact

log = logging.getLogger(__name__)

REF = re.compile(r"^\s*\[?근거\s*(\d+)\]?")
MARKS = re.compile(r"[「」『』\"'“”‘’·ㆍ・…,.()\[\]{}:;!?\-–—/~]")
ELLIPSIS = re.compile(r"(?:\.{3}|…|\(중략\)|\[중략\])")
FRAGMENT_MIN = 8


def _plain(text: str) -> str:
    return MARKS.sub("", compact(text))


def overlap(quote: str, body: str) -> float:
    q, b = compact(quote), compact(body)
    pairs = [q[i : i + 2] for i in range(len(q) - 1)]
    return sum(1 for p in pairs if p in b) / len(pairs) if pairs else 0.0


def normalized_match(quote: str, body: str) -> bool:
    plain_body = _plain(body)
    parts = [p for p in (_plain(f) for f in ELLIPSIS.split(quote)) if len(p) >= FRAGMENT_MIN]
    if not parts:
        return False
    at = 0
    for part in parts:
        found = plain_body.find(part, at)
        if found < 0:
            return False
        at = found + len(part)
    return True


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


Quotes = Callable[[list[dict[str, str]]], Awaitable[list[bool] | None]]


def _mismatched(ledger: Ledger, findings: Findings) -> list[tuple[Finding, Authority]]:
    out: list[tuple[Finding, Authority]] = []
    for item in findings.found:
        authority = _authority(ledger, item.source)
        if authority is not None and not authority.has_quote(item.quote):
            out.append((item, authority))
    return out


def check_research(findings: Findings, ledger: Ledger, relaxed: frozenset[str] = frozenset()) -> Findings:
    def judge(item: Finding) -> Finding | None:
        authority = _authority(ledger, item.source)
        if authority is None:
            log.info("quote_mismatch kind=no_ref source=%r", item.source[:20])
            return None
        if not authority.has_quote(item.quote) and item.quote not in relaxed:
            log.info(
                "quote_mismatch kind=%s ref=%d quote_chars=%d overlap=%.2f normalized=%s",
                authority.kind,
                authority.ref,
                len(item.quote),
                overlap(item.quote, authority.body),
                normalized_match(item.quote, authority.body),
            )
            return None
        return replace(item, source=_shown(authority))

    def ref(source: str) -> str | None:
        authority = _authority(ledger, source)
        return _shown(authority) if authority else None

    return replace(_keep(findings, judge), issues=check_refs(findings.issues, ref))


async def check_research_relaxed(findings: Findings, ledger: Ledger, quotes: Quotes) -> Findings:
    pairs = _mismatched(ledger, findings)
    matched = await quotes([{"body": a.body, "quote": f.quote} for f, a in pairs]) if pairs else None
    relaxed = frozenset(f.quote for (f, _), ok in zip(pairs, matched or [], strict=False) if ok)
    if relaxed:
        log.info("quote_relaxed count=%d of=%d", len(relaxed), len(pairs))
    return check_research(findings, ledger, relaxed)
