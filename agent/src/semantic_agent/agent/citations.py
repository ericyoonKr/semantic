import re
from collections.abc import Callable
from dataclasses import dataclass, replace

from semantic_agent.agent.authority import Ledger, compact

REF = re.compile(r"^\s*\[?근거\s*(\d+)\]?")
UPLOAD_PATH = re.compile(r"/uploads/[^\s·,()\[\]]+")
KG_NODE = re.compile(r"(?<![0-9A-Za-z])L\d+(?![0-9A-Za-z])")


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


def check_document(findings: Findings, file_text: Callable[[str], str]) -> Findings:
    def judge(item: Finding) -> Finding | None:
        match = UPLOAD_PATH.search(item.source)
        wanted = compact(item.quote)
        if match is None or not wanted:
            return None
        return item if wanted in compact(file_text(match.group(0))) else None

    return _keep(findings, judge)


def check_graph(findings: Findings, ledger: Ledger) -> Findings:
    def judge(item: Finding) -> Finding | None:
        nodes = KG_NODE.findall(item.source)
        if not nodes or any(node not in ledger.kg_nodes for node in nodes):
            return None
        return item

    return _keep(findings, judge)
