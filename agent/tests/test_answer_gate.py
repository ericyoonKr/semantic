from semantic_agent.api.answer_gate import NOTICE, SentenceGate, check_sentence
from semantic_agent.ledger import Ledger


def _ledger() -> Ledger:
    ledger = Ledger()
    ledger.add_law(
        {
            "law_name": "국가배상법",
            "unit_key": "제2조",
            "enforcement_date": "2017-10-31",
            "body": "① 국가나 지방자치단체는 공무원이 직무를 집행하면서 손해를 입힌 때에는 배상하여야 한다.\n② 제1항 본문의 경우에 공무원에게 고의 또는 중대한 과실이 있으면 구상할 수 있다.",
        }
    )
    ledger.add_law(
        {
            "law_name": "행정소송법",
            "unit_key": "제20조",
            "enforcement_date": "2014-05-20",
            "body": "① 취소소송은 처분등이 있음을 안 날부터 90일 이내에 제기하여야 한다.\n② 취소소송은 처분등이 있은 날부터 1년을 경과하면 이를 제기하지 못한다.\n③ 제1항의 규정에 의한 기간은 불변기간으로 한다.",
        }
    )
    ledger.add_law(
        {
            "law_name": "개인정보 보호법",
            "unit_key": "제15조",
            "enforcement_date": "2023-09-15",
            "body": "① 개인정보처리자는 다음 각 호의 어느 하나에 해당하는 경우에는 개인정보를 수집할 수 있다.",
        }
    )
    ledger.add_law(
        {
            "law_name": "행정소송법 시행령",
            "unit_key": "제3조",
            "enforcement_date": "2020-01-01",
            "body": "가짜 시행령 본문.",
        }
    )
    ledger.add_case(
        {"court": "대법원", "case_no": "2021두12345", "decided_on": "2022-03-17", "sections": [{"body": "판결 본문"}]}
    )
    ledger.add_case(
        {"court": "헌법재판소", "case_no": "2020헌마1234", "decided_on": "2021-06-24", "sections": [{"body": "결정 본문"}]}
    )
    ledger.trust("판정: 성립하지 않음 — 국가배상법 제5조제1항의 요건 중 영조물 하자가 확인되지 않음")
    return ledger


CORRECT = [
    "국가가 배상책임을 집니다 [근거 1].",
    "공무원에게 중대한 과실이 있으면 구상할 수 있습니다 [근거 1 제2항].",
    "취소소송은 안 날부터 90일 이내에 내야 합니다 [근거 2 제1항].",
    "이 기간은 불변기간입니다 [근거 2 제3항].",
    "국가배상법 제2조는 공무원의 직무상 불법행위를 다룹니다.",
    "국가배상법 제2조 제1항이 기본 규정입니다.",
    "행정소송법 제20조제2항은 1년 기간을 둡니다.",
    "동법 제20조 제3항은 불변기간을 정합니다.",
    "개인정보 보호법 제15조는 수집 요건을 정합니다.",
    "행정소송법 시행령 제3조도 함께 봅니다.",
    "대법원 2021두12345 판결은 이 점을 다룹니다.",
    "헌법재판소 2020헌마1234 결정도 같은 취지입니다.",
    "판정 문장대로 국가배상법 제5조제1항의 요건이 확인되지 않았습니다.",
    "판례는 이렇게 봅니다 [근거 5].",
    "결정례도 있습니다 [근거 6].",
    "이 질문에는 업로드 문서가 필요합니다.",
    "처분일은 2023. 1. 5. 입니다.",
    "계약서 제5조는 해지 사유를 정합니다.",
    "문서의 제3조 제2항을 보면 기한이 있습니다.",
    "2024년 5월 1일에 처분이 있었습니다.",
    "12가지 사유가 적혀 있습니다.",
    "청구 금액은 3000만 원입니다.",
    "같은 법 제20조 제2항도 참고할 만합니다.",
    "국가배상법 제2조와 행정소송법 제20조를 함께 봅니다.",
    "근거 1과 근거 2를 비교하면 다음과 같습니다 [근거 1] [근거 2].",
    "필요한 사실을 알려 주세요?",
    "판정은 결론 유보입니다!",
    "행정소송법 제20조 제1항의 90일은 안 날부터 셉니다.",
    "제1항과 제2항은 서로 다른 기간입니다.",
    "행정소송법상 제20조의 기간을 지켜야 합니다.",
    "국가배상법의 제2조 제2항은 구상을 정합니다.",
    "이 내용은 확인된 근거가 없어 답하지 않습니다.",
]

FABRICATED = [
    "국가가 배상합니다 [근거 9].",
    "구상권이 있습니다 [근거 1 제3항].",
    "취소소송 기간은 [근거 2 제4항]에 있습니다.",
    "민법 제750조가 적용됩니다.",
    "헌법 제10조가 근거입니다.",
    "국가배상법 제3조가 배상 기준을 정합니다.",
    "국가배상법 제2조의2를 보면 됩니다.",
    "국가배상법 제2조 제3항이 적용됩니다.",
    "행정소송법 제19조는 대상을 정합니다.",
    "행정소송법 제20조 제4항도 있습니다.",
    "행정절차법 제21조의 사전통지가 필요합니다.",
    "행정소송법 시행령 제4조도 봅니다.",
    "개인정보 보호법 제17조가 제공 요건입니다.",
    "대법원 2022두9999 판결이 있습니다.",
    "대법원 2019다12345 판결도 같습니다.",
    "헌법재판소 2020헌바99 결정을 보세요.",
    "서울고등법원 2021누54321 판결이 있습니다.",
    "서울행정법원 2022구합12345 판결도 있습니다.",
    "대법원 2021두12346 판결이 같은 취지입니다.",
    "동법 제21조가 적용됩니다.",
    "국가배상법 제5조 제2항도 봅니다.",
    "이 법 제9조에 따라 판단합니다.",
    "행정심판법 제27조의 청구기간이 적용됩니다.",
    "정보공개법 제9조 제1항 제5호가 근거입니다.",
    "국세기본법 제55조가 적용됩니다.",
    "대법원 2010두1234 판결은 반대입니다.",
    "헌재 2019헌가11 결정이 있습니다.",
    "근거로 [근거 0]을 듭니다.",
    "국가배상법 제2조 제1항과 민법 제756조를 함께 봅니다.",
    "대법원 2021두12345 판결과 대법원 2018두55555 판결이 있습니다.",
    "같은 법 제7조도 참고할 만합니다.",
    "같은 법 제99조가 적용됩니다.",
    "대법원 2021스123 판결이 있습니다.",
    "민법상 제750조의 불법행위가 성립합니다.",
    "헌법상 제37조 제2항이 기준입니다.",
]

KNOWN_GAPS: list[str] = []


def test_gate_accuracy_on_sample(capsys: object) -> None:
    ledger = _ledger()
    wrongly_dropped = [s for s in CORRECT if check_sentence(s, ledger).text is None]
    let_through = [s for s in FABRICATED if check_sentence(s, ledger).text is not None]
    gaps_through = [s for s in KNOWN_GAPS if check_sentence(s, ledger).text is not None]
    print(
        f"correct={len(CORRECT)} wrongly_dropped={len(wrongly_dropped)} "
        f"fabricated={len(FABRICATED)} let_through={len(let_through)} "
        f"known_gaps={len(KNOWN_GAPS)} gaps_through={len(gaps_through)}"
    )
    print("wrongly_dropped:", wrongly_dropped)
    print("let_through:", let_through)
    print("gaps_through:", gaps_through)
    assert wrongly_dropped == []
    assert let_through == []


def test_tags_become_code_built_display() -> None:
    ledger = _ledger()
    assert check_sentence("구상할 수 있습니다 [근거 1 제2항].", ledger).text == (
        "구상할 수 있습니다 국가배상법 제2조 제2항(시행 2017-10-31)."
    )
    assert check_sentence("판례가 있습니다 [근거 5].", ledger).text == (
        "판례가 있습니다 대법원 2021두12345 판결(선고 2022-03-17)."
    )


def test_stream_releases_sentences_and_notice_once() -> None:
    gate = SentenceGate(_ledger())
    out: list[str] = []
    for piece in ["국가가 배상", "합니다 [근", "거 1]. 민법 제7", "50조가 적용됩니다. 헌법 제10조도", " 봅니다.\n처분일은 2023. 1. 5. 입니다", "."]:
        out += gate.push(piece)
    out += gate.flush()
    text = "".join(out)
    assert text == f"국가가 배상합니다 국가배상법 제2조(시행 2017-10-31). {NOTICE} \n처분일은 2023. 1. 5. 입니다."
    assert len(gate.drops) == 2


def test_long_sentence_is_checked_at_limit() -> None:
    gate = SentenceGate(_ledger())
    out = gate.push("가" * 450)
    assert out and len(out[0]) == 400


TOOL_LEAKS = [
    ("검토 결과는 조건부 진행입니다 [read_file].", "검토 결과는 조건부 진행입니다."),
    ("필수 수정은 2건입니다 (read_file).", "필수 수정은 2건입니다."),
    ("목차에서 찾았습니다 `grep`.", "목차에서 찾았습니다."),
    ("두 곳에 적혀 있습니다 [read_file, grep].", "두 곳에 적혀 있습니다."),
    ("검색 결과가 없습니다 【search_laws】.", "검색 결과가 없습니다."),
    ("read_file 결과에 따르면 계약 기간은 1년입니다.", "문서 결과에 따르면 계약 기간은 1년입니다."),
    ("kg_find로 찾은 개념은 공무원입니다.", "지식그래프로 찾은 개념은 공무원입니다."),
    ("판단은 [judge] 결과를 따릅니다.", "판단은 결과를 따릅니다."),
    ("조사 결과를 [researcher] 받았습니다.", "조사 결과를 받았습니다."),
    ("파일을 [ls] 보았습니다.", "파일을 보았습니다."),
]

KEEP_AS_IS = [
    "처분서 제목은 「(통지)[과징금 부과]」로 시작합니다.",
    "계약서 · 3쪽에 적혀 있습니다.",
    "처분 사유는 (별지) 형식으로 붙어 있습니다.",
    "제목은 [최종본]입니다.",
    "task force 구성은 문서에 없습니다.",
    "판사(judge)라는 말은 쓰이지 않았습니다.",
]


def test_tool_names_are_scrubbed_and_ordinary_brackets_stay() -> None:
    ledger = _ledger()
    leaked = [(s, check_sentence(s, ledger).text) for s, _ in TOOL_LEAKS]
    wrong = [(s, got) for (s, want), (_, got) in zip(TOOL_LEAKS, leaked) if got != want]
    changed = [s for s in KEEP_AS_IS if check_sentence(s, ledger).text != s]
    print(f"tool leaks={len(TOOL_LEAKS)} not_cleaned={len(wrong)} keep={len(KEEP_AS_IS)} wrongly_changed={len(changed)}")
    print("not_cleaned:", wrong)
    print("wrongly_changed:", changed)
    assert wrong == []
    assert changed == []


def test_notice_says_search_failed_when_it_did() -> None:
    ledger = _ledger()
    ledger.search_failed = True
    gate = SentenceGate(ledger)
    out = "".join(gate.push("민법 제750조가 적용됩니다.\n") + gate.flush())
    assert out.startswith("법령·판례 검색이 지금 되지 않아")


def test_material_wrapper_marks_do_not_reach_the_answer() -> None:
    gate = SentenceGate(_ledger())
    out: list[str] = []
    for piece in ["판단할 수 없습니다. <자료 출", '처="요건 판단">\n', "근거가 없습니다 </자료>.", " 끝입니다."]:
        out += gate.push(piece)
    out += gate.flush()
    text = "".join(out)
    assert "<자료" not in text and "</자료>" not in text
    assert "판단할 수 없습니다." in text and "끝입니다." in text


def test_internal_result_tags_are_removed() -> None:
    from semantic_agent.api.answer_gate import scrub_tools

    assert scrub_tools("판례는 확인되지 않았습니다. [하위 에이전트 결과]") == "판례는 확인되지 않았습니다."
    assert scrub_tools("내용 (조사 결과) 끝") == "내용 끝"


def test_grouped_ref_tags_are_expanded_and_checked() -> None:
    from semantic_agent.api.answer_gate import expand_groups

    assert expand_groups("기준입니다 [근거 1, 근거 2].") == "기준입니다 [근거 1], [근거 2]."
    assert expand_groups("[근거 3, 4 제2항]") == "[근거 3], [근거 4 제2항]"
    assert expand_groups("[근거 5]") == "[근거 5]"
