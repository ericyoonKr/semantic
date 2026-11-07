import logging
import re
from collections.abc import Callable
from dataclasses import dataclass, field

from semantic_agent.ledger import Ledger, article_key, compact
from semantic_agent.turn import Turn

log = logging.getLogger(__name__)

NOTICE = "확인하지 못한 근거를 든 문장을 뺐습니다."
MAX_SENTENCE = 400

TAG = re.compile(r"\[근거\s*(\d+)(?:\s*제\s*(\d+)\s*항)?\]")
LAW = re.compile(
    r"(?<![가-힣])([가-힣]*(?:법률|법))(?:\s*(시행령|시행규칙))?(?:상|의)?\s*(제\s*\d+\s*조(?:\s*의\s*\d+)?)(?:\s*(제\s*(\d+)\s*항))?"
)
SAME_LAW = frozenset({"동법", "같은법", "이법"})
CASE_CODES = (
    "헌마", "헌바", "헌가", "헌나", "헌라", "헌사", "헌아",
    "구합", "구단", "가합", "가단", "가소", "고합", "고단", "고정", "고약",
    "카합", "카단", "드합", "드단", "느합", "느단",
    "구", "누", "두", "다", "나", "가", "도", "노", "므", "르", "스", "브", "마", "라", "카", "재", "추", "후", "허", "그", "아", "모", "오",
)
CASE_NO = re.compile(r"(?<!\d)((?:\d{4}|\d{2})(?:" + "|".join(CASE_CODES) + r")\d{1,7})(?!\d)")


@dataclass(frozen=True)
class Checked:
    text: str | None
    reasons: tuple[str, ...]


def _law_name(name: str, sub: str | None) -> str | None:
    base = compact(name)
    if base in SAME_LAW or base == "법":
        return None
    return base + (sub or "")


def check_sentence(sentence: str, ledger: Ledger) -> Checked:
    reasons: list[str] = []

    def replace(match: re.Match[str]) -> str:
        item = ledger.get(int(match.group(1)))
        paragraph = int(match.group(2)) if match.group(2) else None
        if item is None:
            reasons.append("unknown_ref")
            return match.group(0)
        if paragraph is not None and (item.kind != "law" or paragraph not in item.paragraphs):
            reasons.append("paragraph_not_in_text")
        return item.display(paragraph)

    replaced = TAG.sub(replace, sentence)
    bare = TAG.sub(" ", sentence)
    for match in LAW.finditer(bare):
        name = _law_name(match.group(1), match.group(2))
        article = article_key(match.group(3))
        if article is None:
            continue
        paragraph = int(match.group(5)) if match.group(5) else None
        if ledger.trusts_law(name, article, paragraph):
            continue
        found = ledger.laws_with_article(name, article)
        if not found:
            reasons.append("law_not_read")
        elif paragraph is not None and all(paragraph not in item.paragraphs for item in found):
            reasons.append("paragraph_not_in_text")
    for match in CASE_NO.finditer(bare):
        if not ledger.knows_case(match.group(1)):
            reasons.append("case_not_read")
    if reasons:
        return Checked(None, tuple(dict.fromkeys(reasons)))
    return Checked(replaced, ())


def _cut(buf: str) -> int | None:
    for index, char in enumerate(buf):
        if char == "\n":
            return index + 1
        if char in ".?!" and index > 0 and not buf[index - 1].isdigit():
            if index + 1 < len(buf) and buf[index + 1] in " \t":
                return index + 2
    return None


def _trailing_space(text: str) -> str:
    stripped = text.rstrip()
    return text[len(stripped):]


@dataclass
class SentenceGate:
    ledger: Ledger
    on_drop: Callable[[tuple[str, ...], tuple[int, ...]], None] = lambda reasons, refs: None
    buf: str = ""
    dropped_last: bool = False
    drops: list[tuple[str, ...]] = field(default_factory=list)

    def _emit(self, piece: str) -> list[str]:
        if not piece.strip():
            return [piece]
        checked = check_sentence(piece, self.ledger)
        if checked.text is not None:
            self.dropped_last = False
            return [checked.text]
        self.drops.append(checked.reasons)
        self.on_drop(checked.reasons, tuple(dict.fromkeys(int(m.group(1)) for m in TAG.finditer(piece))))
        if self.dropped_last:
            return [_trailing_space(piece)] if _trailing_space(piece) else []
        self.dropped_last = True
        return [NOTICE + (_trailing_space(piece) or " ")]

    def push(self, text: str) -> list[str]:
        self.buf += text
        out: list[str] = []
        while True:
            cut = _cut(self.buf)
            if cut is None and len(self.buf) > MAX_SENTENCE:
                space = self.buf.rfind(" ", 0, MAX_SENTENCE)
                cut = space + 1 if space > 0 else MAX_SENTENCE
            if cut is None:
                return out
            piece, self.buf = self.buf[:cut], self.buf[cut:]
            out.extend(self._emit(piece))

    def flush(self) -> list[str]:
        piece, self.buf = self.buf, ""
        return self._emit(piece) if piece else []


def log_drop(request_id: str, turn: Turn) -> Callable[[tuple[str, ...], tuple[int, ...]], None]:
    def write(reasons: tuple[str, ...], refs: tuple[int, ...]) -> None:
        log.info("answer_gate drop request_id=%s reasons=%s", request_id, ",".join(reasons))
        turn.block("answer_gate", reasons=list(reasons), refs=list(refs))

    return write
