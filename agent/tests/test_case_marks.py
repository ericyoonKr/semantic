from semantic_agent.tools.search import format_case, format_cases, format_citations

HIT = {
    "decision_id": 13,
    "court": "대법원",
    "case_no": "2019다4444",
    "decided_on": "2019-02-01",
    "field": "civil",
    "section_role": "holding_issue",
    "text": "공무원의 과실 판단 기준.",
    "score": 0.01,
    "en_banc": True,
    "disposition": "reverse_remand",
    "opinion": "majority",
    "direction": "negative",
    "overruled_by": ["2020다1111"],
    "lower_court_view": True,
    "matched": "text+cited",
}


def test_case_hit_shows_marks_in_korean() -> None:
    text = format_cases({"degraded": True, "degraded_reason": "벡터 칸이 비어 있어 글자 검색만 함", "hits": [HIT]})
    head = text.split("\n\n")[1].splitlines()[0]
    assert "[전원합의체] [주문: 파기환송] [변경됨: 2020다1111] [소극] [원심 판단 부분]" in head
    assert "다수의견" not in head
    plain = format_cases({"hits": [{**HIT, "en_banc": False, "overruled_by": [], "direction": None,
                                    "lower_court_view": False, "disposition": "other"}]})
    assert "[" not in plain.split("\n\n")[0].split("]", 1)[1]


def test_read_case_marks_each_section_opinion_and_lower_court_span() -> None:
    data = {
        "court": "대법원",
        "case_no": "2020다1111",
        "decided_on": "2021-03-04",
        "field": "civil",
        "en_banc": True,
        "disposition": "dismiss_appeal",
        "sections": [
            {"role": "holding_issue", "heading": "판시사항", "body": "가", "opinion": "majority", "direction": "positive"},
            {"role": "reasons", "heading": "이유", "body": "나", "opinion": "majority", "lower_court_view": [[0, 12]]},
            {"role": "reasons", "heading": "반대의견", "body": "다", "opinion": "dissent"},
        ],
    }
    lines = format_case(data).split("\n\n")
    assert lines[0].endswith("[전원합의체] [주문: 상고기각]")
    assert lines[1].startswith("## holding_issue — 판시사항 [다수의견] [적극]")
    assert lines[2].startswith("## reasons — 이유 [다수의견] [원심 판단 부분: 글자 0–12]")
    assert lines[3].startswith("## reasons — 반대의견 [반대의견]")


def test_citations_name_law_version_and_citing_cases() -> None:
    cited = format_citations(
        {
            "items": [
                {"law_name": "국가배상법", "law_id": "L1", "unit_key": "제2조", "article_key": "L1:0002",
                 "article_version_id": "L1:0002@2017-01-01", "relation": "cites", "raw": "국가배상법 제2조"},
                {"unit_key": "제9조", "relation": "cites", "raw": "없는법 제9조"},
            ],
            "extract_run": "2:v2",
        }
    ).splitlines()
    assert cited[0] == "- 국가배상법 제2조 · 판본 L1:0002@2017-01-01 · 조문 키 L1:0002 · 원문 「국가배상법 제2조」"
    assert cited[1] == "- 법령 찾지 못함 제9조 · 원문 「없는법 제9조」"
    assert cited[2] == "추출 판: 2:v2"
    citing = format_citations(
        {"items": [{"decision_id": 11, "court": "서울고등법원", "case_no": "2022누2222", "decided_on": "2023-05-06",
                    "unit_key": "제2조제1항", "relation": "cites"}]}
    )
    assert citing == "- 서울고등법원 2022누2222 · 2023-05-06 · decision_id 11 · 인용 제2조제1항"
