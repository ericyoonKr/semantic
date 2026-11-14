from semantic_agent.agent.delegation import read_brief


def test_reread_briefs_differ_from_the_first_brief_on_the_same_issue() -> None:
    first = read_brief("쟁점: 이 사건의 개요\n목적: 조회\n문서 발췌:\n「지방계약법」 제16조에 따라 검사한다.")
    reread = read_brief(
        "쟁점: 이 사건의 개요\n목적: 조회\n먼저 읽을 조문: 지방계약법 제16조\n"
        "근거 없이 쓴 문장(조문·판례를 찾아 붙이거나 뺀다):\n- 총 12건을 집행하였다."
    )
    assert first.issue == reread.issue == "이 사건의 개요"
    assert first.purpose == reread.purpose == "조회"
    assert first.extra == ""
    assert reread.extra.startswith("먼저 읽을 조문: 지방계약법 제16조")
    assert "총 12건을 집행하였다." in reread.extra
    assert read_brief("쟁점: 과태료\n목적: 판단").purpose == "판단"
