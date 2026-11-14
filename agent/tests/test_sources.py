import asyncio

from semantic_agent.api.answer_gate import SentenceGate
from semantic_agent.api.sources import best_line, build_sources, paragraph_span
from semantic_agent.ledger import Ledger

BODY = "제10조(정부위원) ① 국무조정실의 실장 및 차장은 정부위원이 된다.\n② 부·처·청의 처장, 차관, 청장은 정부위원이 된다."


def _ledger() -> Ledger:
    ledger = Ledger()
    ledger.add_law({"law_name": "정부조직법", "unit_key": "제10조", "enforcement_date": "2025-10-01", "body": BODY})
    ledger.add_case({"court": "대법원", "case_no": "2020두1234", "decided_on": "2021-01-01",
                     "sections": [{"body": "가" * 2000 + "\n처분의 취소를 구할 법률상 이익이 없다.\n" + "나" * 2000}]})
    return ledger


def test_paragraph_span_covers_only_that_paragraph() -> None:
    start, end = paragraph_span(BODY, 2) or (0, 0)
    assert BODY[start:end].startswith("②") and BODY[start:end].endswith("된다.")
    assert paragraph_span(BODY, 3) is None


def test_best_line_needs_enough_shared_pieces() -> None:
    start, end = best_line(BODY, "차관과 청장도 정부위원이 된다") or (0, 0)
    assert BODY[start:end].startswith("②")
    assert best_line(BODY, "전혀 다른 말") is None


def test_gate_records_cited_items_and_sources_mark_the_part() -> None:
    ledger = _ledger()
    gate = SentenceGate(ledger)
    out = asyncio.run(gate.push("차관은 정부위원입니다 [근거 1 제2항]. 법률상 이익이 없다고 보았습니다 [근거 2]. "))
    assert out and all("근거" not in piece for piece in out)
    items = {s["ref"]: s for s in build_sources(ledger, gate.cited)}
    law = items[1]
    s, e = law["spans"][0]
    assert law["body"][s:e].startswith("②")
    assert "정부조직법 제10조 제2항(시행 2025-10-01)" in law["displays"]
    case = items[2]
    s, e = case["spans"][0]
    assert "법률상 이익" in case["body"][s:e]
    assert len(case["body"]) < 2000


def test_dropped_sentence_cites_nothing() -> None:
    ledger = _ledger()
    gate = SentenceGate(ledger)
    asyncio.run(gate.push("행정절차법 제21조에 따라야 합니다. "))
    assert gate.cited == []


def test_sources_carry_the_norm_kind_of_the_law() -> None:
    ledger = Ledger()
    item = ledger.add_law({"law_name": "단국대학교 학칙", "unit_key": "제4조", "enforcement_date": "2011-09-26", "body": "① 각 대학의 수업연한은 4년으로 한다.", "version": {"kind": "rule", "version_id": "rule:dku:hakchik:000400"}})
    plain = ledger.add_law({"law_name": "고등교육법", "unit_key": "제23조", "body": "① 학생은 휴학할 수 있다."})
    built = {s["ref"]: s for s in build_sources(ledger, [(item.ref, None, "수업연한은 4년이다"), (plain.ref, None, "휴학할 수 있다")])}
    assert built[item.ref]["norm_kind"] == "rule"
    assert built[plain.ref]["norm_kind"] == ""
