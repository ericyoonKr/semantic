import re
from collections.abc import Callable
from dataclasses import dataclass, replace

from semantic_agent.ledger import Ledger

REF = re.compile(r"^\s*\[?근거\s*(\d+)\]?")


@dataclass(frozen=True)
class Finding:
    claim: str
    source: str
    quote: str
    stance: str = "neutral"


@dataclass(frozen=True)
class Findings:
    found: list[Finding]
    not_found: list[str]
    dropped: int
    limit: str | None
    reverse_query: str | None = None


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


def check_research(findings: Findings, ledger: Ledger) -> Findings:
    def judge(item: Finding) -> Finding | None:
        match = REF.match(item.source)
        authority = ledger.get(int(match.group(1))) if match else None
        if authority is None or not authority.has_quote(item.quote):
            return None
        return replace(item, source=f"[근거 {authority.ref}] {authority.display()}")

    return _keep(findings, judge)
