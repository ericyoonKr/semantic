import re
from typing import Any

from semantic_agent.ledger import CIRCLED, Authority, Ledger, compact

CASE_WINDOW = 700
MIN_SHARED = 4
PARAGRAPH_START = re.compile(r"(?m)^\s*([①-⑳])")
LINE = re.compile(r"[^\n]+")


def _bigrams(text: str) -> set[str]:
    c = compact(text)
    return {c[i : i + 2] for i in range(len(c) - 1)}


def paragraph_span(body: str, paragraph: int) -> tuple[int, int] | None:
    marks = [(CIRCLED[m.group(1)], m.start(1)) for m in PARAGRAPH_START.finditer(body)]
    for i, (number, start) in enumerate(marks):
        if number == paragraph:
            end = marks[i + 1][1] if i + 1 < len(marks) else len(body)
            return start, len(body[:end].rstrip())
    return None


def best_line(body: str, sentence: str) -> tuple[int, int] | None:
    wanted = _bigrams(sentence)
    best: tuple[int, tuple[int, int]] | None = None
    for m in LINE.finditer(body):
        shared = len(wanted & _bigrams(m.group(0)))
        if shared >= MIN_SHARED and (best is None or shared > best[0]):
            best = (shared, (m.start(), m.end()))
    return best[1] if best else None


def _merge(spans: list[tuple[int, int]]) -> list[tuple[int, int]]:
    out: list[tuple[int, int]] = []
    for start, end in sorted(spans):
        if out and start <= out[-1][1]:
            out[-1] = (out[-1][0], max(out[-1][1], end))
        else:
            out.append((start, end))
    return out


def _window(body: str, spans: list[tuple[int, int]]) -> tuple[str, list[tuple[int, int]]]:
    if not spans:
        return body[: CASE_WINDOW * 2], []
    lo = max(0, spans[0][0] - CASE_WINDOW)
    hi = min(len(body), spans[-1][1] + CASE_WINDOW)
    return body[lo:hi], [(s - lo, e - lo) for s, e in spans if s >= lo and e <= hi]


def _item(item: Authority, cites: list[tuple[int | None, str]]) -> dict[str, Any]:
    spans: list[tuple[int, int]] = []
    for paragraph, sentence in cites:
        span = paragraph_span(item.body, paragraph) if paragraph is not None else None
        span = span or best_line(item.body, sentence)
        if span:
            spans.append(span)
    spans = _merge(spans)
    body = item.body
    if item.kind == "case":
        body, spans = _window(body, spans)
    return {
        "ref": item.ref,
        "kind": item.kind,
        "label": item.label,
        "note": item.note,
        "display": item.display(),
        "displays": list(dict.fromkeys([item.display(p) for p, _ in cites if p is not None] + [item.display(), item.label])),
        "body": body,
        "spans": [list(s) for s in spans],
    }


def build_sources(ledger: Ledger, cited: list[tuple[int, int | None, str]]) -> list[dict[str, Any]]:
    by_ref: dict[int, list[tuple[int | None, str]]] = {}
    for ref, paragraph, sentence in cited:
        by_ref.setdefault(ref, []).append((paragraph, sentence))
    out = []
    for ref, cites in by_ref.items():
        item = ledger.get(ref)
        if item is not None:
            out.append(_item(item, cites))
    return out
