use super::{CASE_NOT_READ, Mention, PARAGRAPH_NOT_READ, ReadSet, UNIT_NOT_READ, check, cut, mentions};

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
