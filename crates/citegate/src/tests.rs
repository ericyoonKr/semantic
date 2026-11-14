use super::{CASE_NOT_READ, Mention, NAMED_NOT_READ, ORDINANCE_NOT_READ, PARAGRAPH_NOT_READ, ReadSet, UNIT_NOT_READ, check, cut, mentions, suffixes};

const BODY_2: &str = "① 배상하여야 한다.\n② 구상할 수 있다.";

fn reads() -> ReadSet {
    let mut r = ReadSet::default();
    r.add_unit_body("국가배상법", "제2조", BODY_2);
    r.add_unit("식품위생법 시행규칙", "별표 23");
    r.add_unit("국가를 당사자로 하는 계약에 관한 법률", "제27조");
    r.add_case("대법원 2024두35989");
    r.add_case("2024두35989");
    r
}

#[test]
fn finds_articles_annexes_and_cases_in_many_writings() {
    let m = mentions("「공직자의 이해충돌 방지법」 제5조와 식품위생법 시행규칙 별표 23, 대법원 2024두35989 판결");
    assert!(m.contains(&Mention::Unit { law: Some("공직자의이해충돌방지법".into()), unit: "제5조".into(), para: None }));
    assert!(m.contains(&Mention::Unit { law: Some("식품위생법시행규칙".into()), unit: "별표23".into(), para: None }));
    assert!(m.contains(&Mention::Case("2024두35989".into())));
    assert_eq!(mentions("같은 법 제3조의2")[0], Mention::Unit { law: None, unit: "제3조의2".into(), para: None });
}

#[test]
fn read_units_pass_and_unread_ones_are_blocked() {
    let r = reads();
    let pass = [
        "국가배상법 제2조에 따라 배상한다.",
        "「국가배상법」 제2조 제1항을 보면 된다.",
        "식품위생법 시행규칙 별표 23은 행정처분 기준이다.",
        "별표 23(행정처분 기준)에 있다.",
        "「국가를 당사자로 하는 계약에 관한 법률」 제27조가 근거다.",
        "대법원 2024두35989 판결은 이렇게 보았다.",
        "일반론으로 설명하면 이렇다.",
    ];
    let block = [
        ("「식품위생법」 제7조를 위반했다.", UNIT_NOT_READ),
        ("식품위생법 시행규칙 별표 14는 행정처분 기준이다.", UNIT_NOT_READ),
        ("민법 제750조에 따라 배상한다.", UNIT_NOT_READ),
        ("별표 15에 과태료가 있다.", UNIT_NOT_READ),
        ("대법원 2020두1234 판결이 있다.", CASE_NOT_READ),
    ];
    let wrongly_blocked: Vec<&str> = pass.iter().copied().filter(|s| !check(s, &r).ok).collect();
    let let_through: Vec<&str> = block.iter().filter(|(s, why)| !check(s, &r).reasons.contains(why)).map(|(s, _)| *s).collect();
    assert_eq!(wrongly_blocked, Vec::<&str>::new());
    assert_eq!(let_through, Vec::<&str>::new());
}

#[test]
fn sentences_end_at_marks_or_newlines_but_not_inside_numbers() {
    assert_eq!(cut("첫 문장. 둘째"), Some("첫 문장. ".len()));
    assert_eq!(cut("2020. 1. 1. 시행"), None);
    assert_eq!(cut("줄\n다음"), Some("줄\n".len()));
    assert_eq!(cut("끝나지 않음"), None);
}

fn sample_reads() -> ReadSet {
    let mut r = ReadSet::default();
    r.add_unit_body("국가배상법", "제2조", BODY_2);
    r.add_unit_body("행정소송법", "제20조", "① 90일 이내\n② 1년\n③ 불변기간");
    r.add_unit_body("개인정보 보호법", "제15조", "① 수집할 수 있다.");
    r.add_unit_body("행정소송법 시행령", "제3조", "가짜 시행령 본문.");
    r.add_case("2021두12345");
    r.add_case("2020헌마1234");
    r.add_mentions("판정: 성립하지 않음 — 국가배상법 제5조제1항의 요건 중 영조물 하자가 확인되지 않음");
    r
}

const SAMPLE_PASS: [&str; 24] = [
    "국가배상법 제2조는 공무원의 직무상 불법행위를 다룹니다.",
    "국가배상법 제2조 제1항이 기본 규정입니다.",
    "행정소송법 제20조제2항은 1년 기간을 둡니다.",
    "동법 제20조 제3항은 불변기간을 정합니다.",
    "개인정보 보호법 제15조는 수집 요건을 정합니다.",
    "행정소송법 시행령 제3조도 함께 봅니다.",
    "대법원 2021두12345 판결은 이 점을 다룹니다.",
    "헌법재판소 2020헌마1234 결정도 같은 취지입니다.",
    "판정 문장대로 국가배상법 제5조제1항의 요건이 확인되지 않았습니다.",
    "이 질문에는 업로드 문서가 필요합니다.",
    "처분일은 2023. 1. 5. 입니다.",
    "계약서 제5조는 해지 사유를 정합니다.",
    "문서의 제3조 제2항을 보면 기한이 있습니다.",
    "2024년 5월 1일에 처분이 있었습니다.",
    "12가지 사유가 적혀 있습니다.",
    "청구 금액은 3000만 원입니다.",
    "같은 법 제20조 제2항도 참고할 만합니다.",
    "국가배상법 제2조와 행정소송법 제20조를 함께 봅니다.",
    "행정소송법 제20조 제1항의 90일은 안 날부터 셉니다.",
    "제1항과 제2항은 서로 다른 기간입니다.",
    "행정소송법상 제20조의 기간을 지켜야 합니다.",
    "국가배상법의 제2조 제2항은 구상을 정합니다.",
    "「국가배상법」 제2조 제2항에 따라 구상합니다.",
    "이 내용은 확인된 근거가 없어 답하지 않습니다.",
];

const SAMPLE_BLOCK: [&str; 27] = [
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
    "국가배상법 제2조 제1항과 민법 제756조를 함께 봅니다.",
    "「식품위생법」 제7조 위반입니다.",
    "민법상 제750조의 불법행위가 성립합니다.",
];

#[test]
fn sample_accuracy() {
    let r = sample_reads();
    let wrongly_blocked: Vec<&str> = SAMPLE_PASS.iter().copied().filter(|s| !check(s, &r).ok).collect();
    let let_through: Vec<&str> = SAMPLE_BLOCK.iter().copied().filter(|s| check(s, &r).ok).collect();
    println!(
        "pass_sample={} wrongly_blocked={} block_sample={} let_through={}",
        SAMPLE_PASS.len(),
        wrongly_blocked.len(),
        SAMPLE_BLOCK.len(),
        let_through.len()
    );
    assert_eq!(wrongly_blocked, Vec::<&str>::new());
    assert_eq!(let_through, Vec::<&str>::new());
    assert!(check("국가배상법 제2조 제3항이 적용됩니다.", &r).reasons.contains(&PARAGRAPH_NOT_READ));
}

#[test]
fn annexes_pass_only_when_read_or_listed() {
    let mut r = ReadSet::default();
    r.add_unit("식품위생법 시행규칙", "별표 1");
    r.add_unit("식품위생법 시행규칙", "별표 23");
    let pass = [
        "식품위생법 시행규칙 별표 1에 위생적 취급 기준이 있습니다.",
        "별표 1에는 취급 기준이 있습니다.",
        "식품위생법 시행규칙 별표 23은 행정처분 기준입니다.",
        "별표 23(행정처분 기준)을 보면 됩니다.",
        "별지 제1호서식으로 신청합니다.",
    ];
    let block = [
        "식품위생법 시행규칙 별표 14는 행정처분 기준입니다.",
        "별표 15는 과태료 부과기준입니다.",
        "식품위생법 시행령 별표 1은 영업의 종류입니다.",
        "식품위생법 시행규칙 별표 1의2에 따릅니다.",
        "별표 2에 따라 판단합니다.",
    ];
    let wrongly_blocked: Vec<&str> = pass.iter().copied().filter(|s| !check(s, &r).ok).collect();
    let let_through: Vec<&str> = block.iter().copied().filter(|s| check(s, &r).ok).collect();
    assert_eq!(wrongly_blocked, Vec::<&str>::new());
    assert_eq!(let_through, Vec::<&str>::new());
}

#[test]
fn articles_away_from_the_law_name_take_the_name_before_them() {
    let listed = "「지방자치단체를 당사자로 하는 계약에 관한 법률」(지방계약법): 제16조, 제17조 및 제18조가 언급되었습니다.";
    let mut r = ReadSet::default();
    r.add_unit("지방자치단체를 당사자로 하는 계약에 관한 법률", "제16조");
    assert_eq!(
        check(listed, &r).unread,
        vec!["지방자치단체를당사자로하는계약에관한법률 제17조".to_string(), "지방자치단체를당사자로하는계약에관한법률 제18조".to_string()]
    );
    r.add_unit("지방자치단체를 당사자로 하는 계약에 관한 법률", "제17조");
    r.add_unit("지방자치단체를 당사자로 하는 계약에 관한 법률", "제18조");
    assert!(check(listed, &r).ok);
    let empty = ReadSet::default();
    let pass = [
        "처분서 제목은 「(통지)[과징금 부과]」로 시작하고 제3조를 인용합니다.",
        "계약서 제5조는 해지 사유를 정합니다.",
        "「▼▼군 수의계약 운영계획」의 제2항을 봅니다.",
    ];
    let wrongly_blocked: Vec<&str> = pass.iter().copied().filter(|s| !check(s, &empty).ok).collect();
    assert_eq!(wrongly_blocked, Vec::<&str>::new());
}

#[test]
fn naming_a_law_needs_some_of_it_read_and_passing_sentences_carry_their_citations() {
    let mut r = ReadSet::default();
    r.add_unit_body("지방자치단체를 당사자로 하는 계약에 관한 법률", "제16조", "① 감독한다.");
    r.add_case("2024두35989");
    let v = check("「지방공무원법」상 성실 의무 위반에 해당할 수 있습니다.", &r);
    assert!(!v.ok && v.reasons.contains(&super::LAW_NOT_READ));
    assert_eq!(v.unread, vec!["지방공무원법".to_string()]);
    assert!(check("「지방자치단체를 당사자로 하는 계약에 관한 법률」에 따른 감독입니다.", &r).ok);
    let v = check("「지방자치단체를 당사자로 하는 계약에 관한 법률」 제16조 제1항과 대법원 2024두35989 판결", &r);
    assert!(v.ok);
    assert_eq!(
        v.cites,
        vec![
            super::Cite { law: "지방자치단체를당사자로하는계약에관한법률".into(), unit: "제16조".into(), para: Some(1) },
            super::Cite { law: String::new(), unit: "2024두35989".into(), para: None },
        ]
    );
    assert!(check("「▼▼군 수의계약 운영계획」에 따른다.", &ReadSet::default()).ok);
}

#[test]
fn footnote_numbers_and_page_marks_do_not_change_the_law_name() {
    let mut r = ReadSet::default();
    r.add_unit("공무원 행동강령", "제2조");
    r.add_unit("공직자의 이해충돌 방지법", "제5조");
    let pass = [
        "「공무원 행동강령57)」 제2조에 따른다.",
        "공무원 행동강령57) 제2조에 따른다.",
        "「공직자의 이해충---p3---돌 방지법」 제5조를 어겼다.",
    ];
    let wrongly_blocked: Vec<&str> = pass.iter().copied().filter(|s| !check(s, &r).ok).collect();
    assert_eq!(wrongly_blocked, Vec::<&str>::new());
    assert_eq!(super::clean("1)에서 2)로 바뀐다"), "1)에서 2)로 바뀐다");
}

fn named(sentence: &str) -> Vec<String> {
    mentions(sentence).into_iter().filter_map(|m| if let Mention::Named(n) = m { Some(n) } else { None }).collect()
}

#[test]
fn bare_law_names_are_found_with_the_words_before_them_but_not_inside_citations() {
    let found = named("클럽이 식품위생법에 따라 영업정지를 받고 경범죄 처벌법으로도 처벌됩니다.");
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0], "클럽이 식품위생법");
    assert!(found[1].ends_with("경범죄 처벌법"), "{found:?}");
    assert_eq!(named("마약류 관리에 관한 법률에 따라 처벌합니다."), vec!["마약류 관리에 관한 법률".to_string()]);
    assert!(named("「식품위생법」 제75조와 식품위생법 시행규칙 별표 23에 따릅니다.").is_empty());
    assert!(named("같은 법에 따르고, 이 법 제3조와 동법 규정을 봅니다.").is_empty());
    assert_eq!(named("관련 법령에 따라 처리합니다."), vec!["관련 법령".to_string()]);
    assert!(named("법률 검토를 합니다.").is_empty());
    let found = named("대법원 판결과 헌법재판소 결정, 불법행위와 방법을 봅니다.");
    assert!(found.len() == 1 && found[0].ends_with("방법"), "{found:?}");
    assert_eq!(suffixes("이 사건에서 경범죄 처벌법"), vec!["이사건에서경범죄처벌법", "사건에서경범죄처벌법", "경범죄처벌법", "처벌법"]);
    assert_eq!(suffixes("식품위생법령"), vec!["식품위생법령", "식품위생법"]);
}

#[test]
fn ordinances_named_with_a_region_are_blocked_unless_the_sentence_says_they_are_not_held() {
    let mut r = ReadSet::default();
    r.add_unit_body("지방자치법", "제28조", "① 조례를 제정할 수 있다.");
    let pass = [
        "조례(자치법규)는 이 서비스의 자료에 없어 확인하지 못했습니다.",
        "강남구 조례는 자료에 없어 확인하지 못했습니다.",
        "조례 제정 권한은 지방자치단체에 있습니다.",
        "지방자치단체의 조례로 정한다.",
        "관련 조례에 따릅니다.",
        "식품위생 관련 조례 유형은 다음과 같습니다.",
        "자치법규 체계를 설명합니다.",
        "지방자치법 제28조에 따라 조례를 정합니다.",
        "구청장은 조례를 집행합니다.",
        "서울특별시 조례는 확인하지 못했습니다.",
    ];
    let block = [
        "강남구 조례 제5조에 따라 영업정지합니다.",
        "서울특별시 식품위생 조례는 영업정지를 정합니다.",
        "강남구 자치법규에 따릅니다.",
        "경기도 조례로 과태료를 부과합니다.",
        "해운대구 유흥주점 관리 조례가 적용됩니다.",
        "부산광역시 조례에 근거합니다.",
        "서울시 조례상 의무입니다.",
        "성남시 단란주점 조례를 위반했습니다.",
        "강남구 조례(식품위생)에 따릅니다.",
        "제주특별자치도 조례가 근거입니다.",
        "이 사건의 제재 근거가 되는 조례는 없습니다.",
        "그런 조례는 존재하지 않습니다.",
        "관련 자치법규는 없는 것으로 보입니다.",
    ];
    let wrongly_blocked: Vec<&str> = pass.iter().copied().filter(|s| !check(s, &r).ok).collect();
    let let_through: Vec<&str> = block.iter().filter(|s| !check(s, &r).reasons.contains(&ORDINANCE_NOT_READ)).copied().collect();
    assert_eq!(wrongly_blocked, Vec::<&str>::new());
    assert_eq!(let_through, Vec::<&str>::new());
}

const PLAIN: [&str; 30] = [
    "어느 지역인지 알려 주시겠어요?",
    "업소가 위치한 지역을 알려 주세요.",
    "조례는 우리 자료에 없어 확인하지 못했습니다.",
    "정리하자면 다음과 같습니다.",
    "업종이 무엇인가요?",
    "클럽은 서울 강남구에 있습니다.",
    "사건은 2024년 5월에 일어났습니다.",
    "아래에서 차례로 설명합니다.",
    "먼저 사실관계를 정리합니다.",
    "두 가지 길이 있습니다.",
    "신고는 구청 위생과에 합니다.",
    "질문의 취지를 이렇게 이해했습니다.",
    "근거 원문은 오른쪽에서 볼 수 있습니다.",
    "추가로 확인할 사실이 있습니다.",
    "답변을 짧게 줄였습니다.",
    "이 답은 올린 문서를 바탕으로 합니다.",
    "날짜를 알려 주시면 더 정확히 볼 수 있습니다.",
    "처분서를 받으셨나요?",
    "과태료 통지를 받으신 날짜가 언제인가요?",
    "필요한 서류는 세 가지입니다.",
    "요약하면 이렇습니다.",
    "표로 정리하면 다음과 같습니다.",
    "감사합니다.",
    "네, 말씀하세요.",
    "이 대화에서 읽은 근거는 두 개입니다.",
    "판례는 우리 자료에 없어 확인하지 못했습니다.",
    "상황을 조금 더 설명해 주세요.",
    "다음 단계로 넘어가겠습니다.",
    "앞서 드린 답을 다시 정리합니다.",
    "이 부분은 사실 확인이 먼저입니다.",
];

const CLAIMS: [&str; 30] = [
    "식품위생법에 따라 영업정지 처분을 받을 수 있습니다.",
    "업주는 관리 의무를 위반했습니다.",
    "방조죄로 처벌될 수 있습니다.",
    "과태료가 부과됩니다.",
    "허가가 취소될 수 있습니다.",
    "조례는 법률의 위임이 있어야 합니다.",
    "그런 조례는 존재하지 않습니다.",
    "이 경우 형사 처벌 대상입니다.",
    "업소에 과징금이 부과될 수 있습니다.",
    "영업자에게는 안전 관리 의무가 있습니다.",
    "해당 행위는 위반에 해당합니다.",
    "이 조항이 적용됩니다.",
    "업주에게 손해배상 책임이 있습니다.",
    "출입 금지 명령이 내려질 수 있습니다.",
    "벌금형이 선고될 수 있습니다.",
    "허가 없이 영업하면 처벌받습니다.",
    "영업정지 15일 처분이 기준입니다.",
    "위반 횟수에 따라 처분이 무거워집니다.",
    "신고 의무를 다하지 않으면 과태료입니다.",
    "청소년 출입은 금지됩니다.",
    "관리 책임을 물을 수 있습니다.",
    "이 사안에는 형법이 적용됩니다.",
    "업소 폐쇄 처분도 가능합니다.",
    "방치는 묵인으로 보아 처벌됩니다.",
    "구청장은 영업 허가를 취소할 수 있습니다.",
    "위반 사실이 확인되면 처분합니다.",
    "그 조례는 존재하지 않는다고 봅니다.",
    "책임은 업주에게 있습니다.",
    "2차 위반이면 영업정지 기간이 늘어납니다.",
    "불법 행위를 방치한 책임을 집니다.",
];

#[test]
fn quotes_lifted_from_the_body_match_even_with_marks_or_gaps_but_made_up_ones_do_not() {
    let body = "① 식품의약품안전처장 또는 시장·군수·구청장은 영업자가 다음 각 호의 어느 하나에 해당하는 경우에는 영업허가를 취소하거나 6개월 이내의 기간을 정하여 영업의 전부 또는 일부를 정지할 수 있다.\n② 제1항에 따른 처분의 세부 기준은 총리령으로 정한다.";
    let pass = [
        "시장ㆍ군수ㆍ구청장은 영업자가 다음 각 호의 어느 하나에 해당하는 경우",
        "영업허가를 취소하거나 ... 영업의 전부 또는 일부를 정지할 수 있다.",
        "「영업허가를 취소하거나」 6개월 이내의 기간을 정하여",
        "영업의 전부 또는 일부를 정지할 수 있다. 제1항에 따른 처분의 세부 기준은 총리령으로 정한다",
        "영업자가 다음 각 호의 어느 하나에 해당하는 경우에는 영업허가를 취소하거나 6개월 이내의 기간을 정하여 영업을 정지할 수 있다",
    ];
    let block = [
        "영업자는 과태료를 내야 한다",
        "구청장은 영업자에게 1년 이내의 영업정지를 명한다",
        "짧은 글",
        "영업자가 청소년에게 주류를 판매한 경우 영업허가를 취소한다",
    ];
    let wrongly_blocked: Vec<&str> = pass.iter().copied().filter(|q| !super::quote_match(body, q)).collect();
    let let_through: Vec<&str> = block.iter().copied().filter(|q| super::quote_match(body, q)).collect();
    assert_eq!(wrongly_blocked, Vec::<&str>::new());
    assert_eq!(let_through, Vec::<&str>::new());
}

#[test]
fn legal_effect_claims_are_told_apart_from_plain_sentences() {
    let false_alarms: Vec<&str> = PLAIN.iter().copied().filter(|s| super::asserts_law(s)).collect();
    let missed: Vec<&str> = CLAIMS.iter().copied().filter(|s| !super::asserts_law(s)).collect();
    println!("plain_sample={} false_alarms={} claim_sample={} missed={}", PLAIN.len(), false_alarms.len(), CLAIMS.len(), missed.len());
    assert!(false_alarms.len() <= 2, "{false_alarms:?}");
    assert!(missed.len() <= 2, "{missed:?}");
}

#[test]
fn bare_names_of_read_laws_pass_with_a_law_cite_and_unread_ones_are_reported() {
    let mut r = ReadSet::default();
    r.add_unit_body("식품위생법", "제75조", "① 영업정지를 명할 수 있다.");
    let v = check("클럽이 식품위생법에 따라 영업정지 처분을 받을 수 있습니다.", &r);
    assert!(v.ok, "{v:?}");
    assert_eq!(v.cites, vec![super::Cite { law: "식품위생법".into(), unit: String::new(), para: None }]);
    let v = check("경범죄 처벌법으로도 처벌됩니다.", &r);
    assert!(!v.ok && v.reasons == vec![NAMED_NOT_READ]);
    assert_eq!(v.named, vec!["경범죄 처벌법".to_string()]);
    let v = check("해결방법은 식품위생법령을 보는 것입니다.", &r);
    assert_eq!(v.named, vec!["해결방법".to_string()]);
    assert_eq!(v.cites.len(), 1);
}

#[test]
fn short_names_count_as_the_law_that_was_read() -> Result<(), lawnames::Error> {
    let entry = lawnames::Entry {
        alias: "지방계약법".into(),
        law_id: "010002".into(),
        name: "지방자치단체를 당사자로 하는 계약에 관한 법률".into(),
        source: "시험".into(),
    };
    lawnames::install(lawnames::Aliases::from_entries(vec![entry])?);
    let mut r = ReadSet::default();
    r.add_unit_body("지방자치단체를 당사자로 하는 계약에 관한 법률", "제16조", "① 감독하여야 한다.");
    r.add_unit("지방자치단체를 당사자로 하는 계약에 관한 법률 시행령", "제64조");
    let pass = [
        "지방계약법 제16조에 따라 감독해야 합니다.",
        "「지방계약법」 제16조 제1항이 근거입니다.",
        "지방계약법 시행령 제64조에 따릅니다.",
    ];
    let block = ["지방계약법 제18조에 따라 대가를 지급합니다.", "지방계약법 제16조 제2항이 근거입니다."];
    let wrongly_blocked: Vec<&str> = pass.iter().copied().filter(|s| !check(s, &r).ok).collect();
    let let_through: Vec<&str> = block.iter().copied().filter(|s| check(s, &r).ok).collect();
    assert_eq!(wrongly_blocked, Vec::<&str>::new());
    assert_eq!(let_through, Vec::<&str>::new());
    Ok(())
}

#[test]
fn page_references_to_uploaded_documents_are_recognised() {
    let pass = [
        "총 12건을 목적 외로 집행하였습니다 「감사보고서.pdf · 25쪽」.",
        "허가 구역을 벗어나 사용하였습니다 [보고서.docx · 9쪽].",
        "두 쪽에 걸쳐 적혀 있습니다 「문서.hwpx · 3-4쪽」.",
        "「처분서.PDF · 2, 3쪽」에 따르면 사전통지가 없었습니다.",
    ];
    let block = ["25쪽에 적혀 있습니다.", "「지방계약법」 제16조에 따라 감독합니다.", "문서.pdf를 보세요.", "「문서.pdf」 25쪽입니다."];
    let missed: Vec<&str> = pass.iter().copied().filter(|s| !super::cites_document(s)).collect();
    let wrongly: Vec<&str> = block.iter().copied().filter(|s| super::cites_document(s)).collect();
    assert_eq!(missed, Vec::<&str>::new());
    assert_eq!(wrongly, Vec::<&str>::new());
}

#[test]
fn school_rules_are_checked_like_laws() {
    let mut r = ReadSet::default();
    r.add_unit_body("단국대학교 학칙", "제23조", "① 휴학은 학기 단위로 한다.");
    assert!(check("「단국대학교 학칙」 제23조에 따라 휴학은 학기 단위로 합니다.", &r).ok);
    assert!(check("단국대학교 학칙 제23조에 따라 휴학할 수 있습니다.", &r).ok);
    assert!(!check("단국대학교 학칙 제24조에 따라 복학합니다.", &r).ok);
    assert!(check("단국대학교 학칙에 따르면 휴학은 학기 단위입니다.", &r).ok);
    let v = check("단국대학교 학칙에 따르면 휴학은 학기 단위입니다.", &ReadSet::default());
    assert!(v.reasons.contains(&NAMED_NOT_READ), "{v:?}");
    assert_eq!(v.named, vec!["단국대학교 학칙".to_string()]);
    let s = check("「단국대학교 학칙 시행세칙」 제5조에 따릅니다.", &ReadSet::default());
    assert!(!s.ok && s.reasons.contains(&UNIT_NOT_READ), "{s:?}");
}

#[test]
fn quotes_survive_headings_revision_tags_parenthetical_edits_and_table_cells() {
    let article = "① 고등학교 이상의 학교의 장은 징집 등에 의하여 입영하는 학생에 대해서는 입영과 동시에 휴학하게 한다. <개정 2021.12.7>\n1. 정보주체의 동의를 받은 경우\n2. 법률에 특별한 규정이 있는 경우";
    let paragraph = "3. 제22조제1항부터 제3항까지(제26조제8항에 따라 준용되는 경우를 포함한다)를 위반하여 동의를 받은 자는 과태료를 부과한다";
    let table = "┌──────┬───┬────┐\n│1. 최초 음주운전을 한 경우│││\n│  가. 자전거등 음주운전의 경우│경징계│감봉∼견책│\n│  나. 자전거등 외 음주운전의 경우│경징계 또는│정직∼감봉│\n│││중징계││\n│    2) 혈중알코올농도가 0.08퍼센트 이상 0.2퍼│중징계│강등∼정직│\n│센트 미만인 경우│││\n│    3) 혈중알코올농도가 0.2퍼센트 이상인 경우│중징계│해임∼정직│\n├──────┼───┼────┤\n│2. 2회 음주운전을 한 경우│중징계│파면∼강등│\n├──────┼───┼────┤\n│3. 3회 이상 음주운전을 한 경우│중징계│파면∼해임│\n└──────┴───┴────┘";
    let pass = [
        (article, "제73조(복학보장 및 군복무 중 학점취득 인정) ① 고등학교 이상의 학교의 장은 징집 등에 의하여 입영하는 학생에 대해서는 입영과 동시에 휴학하게 한다."),
        (article, "입영과 동시에 휴학하게 한다.\n1. 정보주체의 동의를 받은 경우\n2. 법률에 특별한 규정이 있는 경우"),
        (paragraph, "3. 제22조제1항부터 제3항까지(제26조제8항을 포함한다)를 위반하여 동의를 받은 자는 과태료를 부과한다"),
        (table, "2. 2회 음주운전을 한 경우 | 중징계 | 파면∼강등\n3. 3회 이상 음주운전을 한 경우 | 중징계 | 파면∼해임"),
        (table, "2. 2회 음주운전을 한 경우          중징계   파면∼강등"),
        (table, "나. 자전거등 외 음주운전의 경우 경징계 또는 중징계 정직∼감봉"),
        (table, "최초 음주운전을 한 경우\n  나. 자전거등 외 음주운전의 경우  경징계 또는 중징계  정직∼감봉\n2. 2회 음주운전을 한 경우  중징계  파면∼강등"),
        (table, "2) 혈중알코올농도가 0.08퍼센트 이상 0.2퍼센트 미만인 경우 | 중징계 | 강등∼정직"),
    ];
    let block = [
        (article, "학교의 장은 입영한 학생의 휴학을 거부할 수 있으며 복무 중 취득한 학점은 인정하지 아니한다."),
        (table, "2회 음주운전을 한 경우 경징계 견책, 3회 이상 음주운전을 한 경우 감봉"),
        (table, "2. 2회 음주운전을 한 경우 | 경징계 | 견책\n3. 3회 이상 음주운전을 한 경우 | 경징계 | 감봉"),
        (table, "3) 혈중알코올농도가 0.2퍼센트 이상인 경우 | 경징계 | 감봉∼견책"),
        (paragraph, "동의 없이 개인정보를 수집한 자에게는 3천만원 이하의 과태료를 부과하며 영업을 정지할 수 있다."),
        (table, "최초 음주운전을 한 경우 혈중알코올농도와 관계없이 파면 또는 해임한다."),
    ];
    let wrongly_blocked: Vec<&str> = pass.iter().filter(|(b, q)| !super::quote_match(b, q)).map(|(_, q)| *q).collect();
    let let_through: Vec<&str> = block.iter().filter(|(b, q)| super::quote_match(b, q)).map(|(_, q)| *q).collect();
    assert_eq!(wrongly_blocked, Vec::<&str>::new());
    assert_eq!(let_through, Vec::<&str>::new());
}
