from semantic_agent.agent.citations import normalized_match, overlap

BODY = "① 식품의약품안전처장 또는 시장·군수·구청장은 영업자가 다음 각 호의 어느 하나에 해당하는 경우에는 영업허가를 취소하거나 6개월 이내의 기간을 정하여 영업의 전부 또는 일부를 정지할 수 있다."


def test_quotes_that_differ_only_in_marks_or_ellipsis_still_match() -> None:
    assert normalized_match("시장ㆍ군수ㆍ구청장은 영업자가 다음 각 호의 어느 하나에 해당하는 경우", BODY)
    assert normalized_match("영업허가를 취소하거나 ... 영업의 전부 또는 일부를 정지할 수 있다.", BODY)
    assert normalized_match("「영업허가를 취소하거나」 6개월 이내의 기간을 정하여", BODY)
    assert not normalized_match("영업자는 과태료를 내야 한다", BODY)
    assert not normalized_match("일부를 정지할 수 있다. 영업허가를 취소하거나", BODY)
    assert overlap("영업허가를 취소하거나 영업을 정지할 수 있다", BODY) > 0.8
    assert overlap("아무 관계 없는 글귀입니다", BODY) < 0.3
