import logging
import re
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field

from semantic_agent.ledger import Ledger, article_key, compact
from semantic_agent.turn import Turn

log = logging.getLogger(__name__)

NOTICE = "확인하지 못한 근거를 든 문장을 뺐습니다."
NOTICE_SEARCH = "법령·판례 검색이 지금 되지 않아 근거를 확인하지 못한 문장을 뺐습니다."
TOOL_LABELS = {
    "read_file": "문서", "write_file": "메모", "edit_file": "메모", "grep": "문서", "glob": "문서", "ls": "문서",
    "search_laws": "법령 검색", "search_cases": "판례 검색", "read_law": "법령", "read_case": "판례",
    "case_citations": "인용 관계", "kg_find": "지식그래프", "kg_expand": "지식그래프",
    "kg_write_state": "지식그래프", "kg_judge_tree": "지식그래프", "judge": "요건 판단",
    "resolve_version": "판본 결정", "resolve_citations": "인용 해소", "trace_judgment_versions": "판본 대조",
    "task": "조사", "write_todos": "계획", "researcher": "조사",
}
_NAMES = "|".join(sorted(TOOL_LABELS, key=len, reverse=True))
TOOL_TAG = re.compile(r"(?:^|\s+)[\[(（【`]\s*(?:" + _NAMES + r")(?:\s*[,·/]\s*(?:" + _NAMES + r"))*\s*[\])）】`]")
TOOL_BARE = re.compile(r"(?<![A-Za-z0-9_])(" + "|".join(n for n in TOOL_LABELS if "_" in n) + r")(?![A-Za-z0-9_])")


INTERNAL_TAG = re.compile(r"\s*[\[(（【]\s*(?:하위\s*에이전트\s*결과|researcher\s*결과|조사\s*결과)\s*[\])）】]")


TOOL_NAME = re.compile(_NAMES)
HANGUL = re.compile(r"[가-힣]")


def _tag_or_label(match: re.Match[str]) -> str:
    rest = match.string[match.end() : match.end() + 1]
    if not HANGUL.match(rest):
        return ""
    labels = dict.fromkeys(TOOL_LABELS[n] for n in TOOL_NAME.findall(match.group(0)))
    lead = " " if match.group(0)[:1].isspace() else ""
    return lead + "·".join(labels)


def scrub_tools(text: str) -> str:
    text = INTERNAL_TAG.sub("", text)
    text = TOOL_TAG.sub(_tag_or_label, text)
    return TOOL_BARE.sub(lambda m: TOOL_LABELS[m.group(1)], text)
MAX_SENTENCE = 400

TAG = re.compile(r"\[근거\s*(\d+)(?:\s*제\s*(\d+)\s*항)?\]")
TAG_GROUP = re.compile(r"\[\s*근거\s*\d+(?:\s*제\s*\d+\s*항)?(?:\s*[,·]\s*(?:근거\s*)?\d+(?:\s*제\s*\d+\s*항)?)+\s*\]")
TAG_PART = re.compile(r"(\d+)(?:\s*제\s*(\d+)\s*항)?")


def expand_groups(text: str) -> str:
    def one(match: re.Match[str]) -> str:
        parts = TAG_PART.findall(match.group(0))
        return ", ".join(f"[근거 {n} 제{p}항]" if p else f"[근거 {n}]" for n, p in parts)

    return TAG_GROUP.sub(one, text)
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
ANNEX = re.compile(
    r"(?<![가-힣])(?:([가-힣]*(?:법률|법))(?:\s*(시행령|시행규칙))?(?:의|상)?\s*)?(별표|서식)\s*(\d+)(?:\s*의\s*(\d+))?(?!\d)"
)
CASE_NO = re.compile(r"(?<!\d)((?:\d{4}|\d{2})(?:" + "|".join(CASE_CODES) + r")\d{1,7})(?!\d)")


@dataclass(frozen=True)
class Checked:
    text: str | None
    reasons: tuple[str, ...]
    cites: tuple[tuple[int, int | None], ...] = ()


def _law_name(name: str, sub: str | None) -> str | None:
    base = compact(name)
    if base in SAME_LAW or base == "법":
        return None
    return base + (sub or "")


def check_sentence(sentence: str, ledger: Ledger) -> Checked:
    reasons: list[str] = []
    cites: list[tuple[int, int | None]] = []

    def replace(match: re.Match[str]) -> str:
        item = ledger.get(int(match.group(1)))
        paragraph = int(match.group(2)) if match.group(2) else None
        if item is None:
            reasons.append("unknown_ref")
            return match.group(0)
        if paragraph is not None and (item.kind != "law" or paragraph not in item.paragraphs):
            reasons.append("paragraph_not_in_text")
        cites.append((item.ref, paragraph))
        return item.display(paragraph)

    sentence = expand_groups(sentence)
    replaced = TAG.sub(replace, sentence)
    bare = TAG.sub(" ", sentence)
    for match in LAW.finditer(bare):
        article = article_key(match.group(3))
        if article is not None:
            paragraph = int(match.group(5)) if match.group(5) else None
            found = ledger.laws_with_article(_law_name(match.group(1), match.group(2)), article)
            cites.extend((item.ref, paragraph) for item in found)
    for match in ANNEX.finditer(bare):
        name = _law_name(match.group(1), match.group(2)) if match.group(1) else None
        key = match.group(3) + match.group(4) + (f"의{match.group(5)}" if match.group(5) else "")
        cites.extend((item.ref, None) for item in ledger.annexes_named(name, key))
    for match in CASE_NO.finditer(bare):
        cites.extend((item.ref, None) for item in ledger.cases_numbered(match.group(1)))
    if reasons:
        return Checked(None, tuple(dict.fromkeys(reasons)))
    return Checked(scrub_tools(replaced), (), tuple(dict.fromkeys(cites)))


def _cut(buf: str) -> int | None:
    for index, char in enumerate(buf):
        if char == "\n":
            return index + 1
        if char in ".?!" and index > 0 and not buf[index - 1].isdigit():
            if index + 1 < len(buf) and buf[index + 1] in " \t":
                return index + 2
    return None


MATERIAL_MARK = re.compile(r' ?</?자료(?:\s+출처="[^"\n]*")?\s*>')


def _trailing_space(text: str) -> str:
    stripped = text.rstrip()
    return text[len(stripped):]


Checker = Callable[[list[str]], Awaitable[list[bool] | None]]
CITES_SOMETHING = re.compile(r"제\s*\d+\s*조|별표\s*\d|서식\s*\d")
NOT_READ = "not_read"


def offline_verdicts(pieces: list[str]) -> list[bool]:
    return [not (CITES_SOMETHING.search(p) or CASE_NO.search(p)) for p in pieces]


async def unreachable(pieces: list[str]) -> list[bool] | None:
    return None


@dataclass
class SentenceGate:
    ledger: Ledger
    on_drop: Callable[[tuple[str, ...], tuple[int, ...]], None] = lambda reasons, refs: None
    checker: Checker = unreachable
    buf: str = ""
    dropped_last: bool = False
    check_failed: bool = False
    drops: list[tuple[str, ...]] = field(default_factory=list)
    cited: list[tuple[int, int | None, str]] = field(default_factory=list)

    async def _read_verdicts(self, pieces: list[str]) -> list[bool]:
        asks = [p for p in pieces if p.strip()]
        got = await self.checker(asks) if asks else []
        if got is None or len(got) != len(asks):
            self.check_failed = True
            got = offline_verdicts(asks)
        answers = iter(got)
        return [next(answers) if p.strip() else True for p in pieces]

    def _emit(self, piece: str, read: bool) -> list[str]:
        if not piece.strip():
            return [piece]
        checked = check_sentence(piece, self.ledger)
        reasons = checked.reasons if read else (*checked.reasons, NOT_READ)
        if not reasons and checked.text is not None:
            self.dropped_last = False
            self.cited.extend((ref, paragraph, piece) for ref, paragraph in checked.cites)
            return [checked.text]
        self.drops.append(reasons)
        self.on_drop(reasons, tuple(dict.fromkeys(int(m.group(1)) for m in TAG.finditer(piece))))
        if self.dropped_last:
            return [_trailing_space(piece)] if _trailing_space(piece) else []
        self.dropped_last = True
        notice = NOTICE_SEARCH if self.ledger.search_failed or self.check_failed else NOTICE
        return [notice + (_trailing_space(piece) or " ")]

    async def _emit_all(self, pieces: list[str]) -> list[str]:
        pieces = [MATERIAL_MARK.sub("", p) for p in pieces]
        out: list[str] = []
        for piece, read in zip(pieces, await self._read_verdicts(pieces), strict=True):
            out.extend(self._emit(piece, read))
        return out

    async def push(self, text: str) -> list[str]:
        self.buf += text
        pieces: list[str] = []
        while True:
            cut = _cut(self.buf)
            if cut is None and len(self.buf) > MAX_SENTENCE:
                space = self.buf.rfind(" ", 0, MAX_SENTENCE)
                cut = space + 1 if space > 0 else MAX_SENTENCE
            if cut is None:
                break
            pieces.append(self.buf[:cut])
            self.buf = self.buf[cut:]
        return await self._emit_all(pieces) if pieces else []

    async def flush(self) -> list[str]:
        piece, self.buf = self.buf, ""
        return await self._emit_all([piece]) if piece else []


def log_drop(request_id: str, turn: Turn) -> Callable[[tuple[str, ...], tuple[int, ...]], None]:
    def write(reasons: tuple[str, ...], refs: tuple[int, ...]) -> None:
        log.info("answer_gate drop request_id=%s reasons=%s", request_id, ",".join(reasons))
        turn.block("answer_gate", reasons=list(reasons), refs=list(refs))

    return write
