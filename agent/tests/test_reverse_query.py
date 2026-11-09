from semantic_agent.agent.citations import Findings
from semantic_agent.agent.delegation import reverse_ok


def _f(reverse: str | None) -> Findings:
    return Findings(found=[], not_found=[], dropped=0, limit=None, reverse_query=reverse)


def test_reverse_query_matches_ignoring_spacing_and_punctuation_only() -> None:
    called = ["영조물 하자 부정", "국가배상법 제5조"]
    assert reverse_ok(_f("영조물하자 부정"), called)
    assert reverse_ok(_f(" 국가배상법 제5조. "), called)
    assert not reverse_ok(_f("영조물 하자 인정"), called)
    assert not reverse_ok(_f(""), called)
    assert not reverse_ok(_f(None), called)
