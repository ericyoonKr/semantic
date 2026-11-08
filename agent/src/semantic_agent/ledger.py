import re
import unicodedata
from contextvars import ContextVar
from dataclasses import dataclass, field
from typing import Any

CIRCLED = {chr(0x2460 + i): i + 1 for i in range(20)}
PARAGRAPH_MARK = re.compile(r"(?m)^\s*([①-⑳])")
ARTICLE = re.compile(r"제\s*(\d+)\s*조(?:\s*의\s*(\d+))?")
PARAGRAPH = re.compile(r"제\s*(\d+)\s*항")


def normalize(text: str) -> str:
    return " ".join(unicodedata.normalize("NFC", text).split())


def compact(text: str) -> str:
    return "".join(unicodedata.normalize("NFC", text).split())


def article_key(text: str) -> str | None:
    match = ARTICLE.search(text)
    if match is None:
        return None
    return f"{match.group(1)}의{match.group(2)}" if match.group(2) else match.group(1)


def paragraph_numbers(body: str) -> frozenset[int]:
    return frozenset(CIRCLED[m.group(1)] for m in PARAGRAPH_MARK.finditer(body))


@dataclass(frozen=True)
class Authority:
    ref: int
    kind: str
    label: str
    note: str
    body: str
    law_name: str = ""
    article: str | None = None
    paragraphs: frozenset[int] = frozenset()
    court: str = ""
    case_no: str = ""

    def display(self, paragraph: int | None = None) -> str:
        label = f"{self.label} 제{paragraph}항" if paragraph is not None else self.label
        return f"{label}({self.note})" if self.note else label

    def has_quote(self, quote: str) -> bool:
        wanted = compact(quote)
        return bool(wanted) and wanted in compact(self.body)


@dataclass
class Ledger:
    items: dict[int, Authority] = field(default_factory=dict)
    keys: dict[str, int] = field(default_factory=dict)
    queries: list[str] = field(default_factory=list)
    trusted: list[str] = field(default_factory=list)
    search_failed: bool = False
    case_data_missing: str | None = None
    delegations: dict[str, str] = field(default_factory=dict)

    def _add(self, key: str, build: Any) -> Authority:
        ref = self.keys.get(key)
        if ref is not None:
            return self.items[ref]
        ref = len(self.items) + 1
        item = build(ref)
        self.items[ref] = item
        self.keys[key] = ref
        return item

    def add_law(self, data: dict[str, Any]) -> Authority:
        law_name = str(data.get("law_name") or "").strip()
        unit_key = str(data.get("unit_key") or "").strip()
        enforced = str(data.get("enforcement_date") or "").strip()
        body = str(data.get("body") or "")
        label = " ".join(p for p in (law_name, unit_key) if p)
        note = f"시행 {enforced}" if enforced else ""
        key = f"law\x00{compact(law_name)}\x00{compact(unit_key)}\x00{enforced}"
        return self._add(
            key,
            lambda ref: Authority(
                ref=ref,
                kind="law",
                label=label,
                note=note,
                body=body,
                law_name=compact(law_name),
                article=article_key(unit_key),
                paragraphs=paragraph_numbers(body),
            ),
        )

    def add_case(self, data: dict[str, Any]) -> Authority:
        court = str(data.get("court") or "").strip()
        case_no = str(data.get("case_no") or "").strip()
        decided = str(data.get("decided_on") or "").strip()
        sections = data.get("sections") or []
        body = "\n".join(str(s.get("body", "")) for s in sections if isinstance(s, dict))
        label = " ".join(p for p in (court, case_no) if p) + " 판결"
        note = f"선고 {decided}" if decided else ""
        key = f"case\x00{compact(court)}\x00{compact(case_no)}"
        return self._add(
            key,
            lambda ref: Authority(
                ref=ref, kind="case", label=label, note=note, body=body, court=compact(court), case_no=compact(case_no)
            ),
        )

    def record_query(self, query: str) -> None:
        self.queries.append(query)

    def trust(self, text: str) -> None:
        self.trusted.append(compact(text))

    def get(self, ref: int) -> Authority | None:
        return self.items.get(ref)

    def knows_case(self, case_no: str) -> bool:
        wanted = compact(case_no)
        if any(item.kind == "case" and item.case_no == wanted for item in self.items.values()):
            return True
        return any(wanted in text for text in self.trusted)

    def laws_with_article(self, law_name: str | None, article: str) -> list[Authority]:
        return [
            item
            for item in self.items.values()
            if item.kind == "law"
            and item.article == article
            and (law_name is None or item.law_name.endswith(law_name))
        ]

    def trusts_law(self, law_name: str | None, article: str, paragraph: int | None = None) -> bool:
        number, _, branch = article.partition("의")
        tail = f"제{number}조" + (f"의{branch}" if branch else "") + (f"제{paragraph}항" if paragraph else "")
        head = re.escape(law_name) if law_name else ""
        pattern = re.compile(head + re.escape(tail) + r"(?!의\d)")
        return any(pattern.search(text) for text in self.trusted)


LEDGER: ContextVar[Ledger | None] = ContextVar("ledger", default=None)


def current_ledger() -> Ledger:
    ledger = LEDGER.get()
    if ledger is None:
        ledger = Ledger()
        LEDGER.set(ledger)
    return ledger
