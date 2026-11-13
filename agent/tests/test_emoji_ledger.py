from semantic_agent.api.emoji import strip_emoji
from semantic_agent.api.sessions import SessionRegistry


def test_emoji_are_removed_but_legal_marks_stay() -> None:
    assert strip_emoji("💡 실무 조언 ✅ 끝 ⚠️") == " 실무 조언  끝 "
    assert strip_emoji("제1항 ① 국가 → 지방 ※ 참고 「법」") == "제1항 ① 국가 → 지방 ※ 참고 「법」"


def test_one_session_keeps_one_ledger_across_turns() -> None:
    registry = SessionRegistry()
    first = registry.get("s").ledger
    first.add_case({"court": "대법원", "case_no": "2020두1", "sections": []}, "77")
    assert registry.get("s").ledger.knows_case("2020두1")
    assert not registry.get("other").ledger.knows_case("2020두1")
