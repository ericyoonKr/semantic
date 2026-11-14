#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    ByLawType,
    Statute,
    Decree,
    Rule,
    AdminRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BareName {
    Checked,
    RegionOnly,
    QuotedOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormKind {
    pub kind: &'static str,
    pub label: &'static str,
    pub rank: u8,
    pub tails: &'static [&'static str],
    pub words: &'static [&'static str],
    pub bare: BareName,
    pub weight: Weight,
    pub missing_note: Option<&'static str>,
    pub scope_line: Option<&'static str>,
    pub writing_note: Option<&'static str>,
}

pub const NO_ORDINANCE_NOTE: &str = "조례(자치법규)는 이 서비스의 자료에 없어 확인하지 못했습니다.";
pub const ORDINANCE_SCOPE_LINE: &str = "자료 범위: 자치법규(조례·규칙)는 자료에 없음. 상위 법령(법률·시행령·시행규칙·별표)과 판례에서 찾는다.";
pub const ORDINANCE_WRITING_NOTE: &str =
    "조례·규칙은 지방자치단체의 자치법규다. 법률과 구분해 「조례상 …」으로 적고, 상위 법령과 어긋나는 점이 보이면 그것도 적는다.";
pub const RULE_WRITING_NOTE: &str =
    "학칙·세칙 같은 기관 규정은 그 기관 구성원에게만 미치는 자치규범이고 법령이 아니다. 「학칙상 …」·「규정상 …」으로 적어 법률상 근거와 섞지 않고, 상위 법령(고등교육법 등)이 있으면 함께 든다.";

pub const KINDS: [NormKind; 4] = [
    NormKind {
        kind: "law",
        label: "법령",
        rank: 1,
        tails: &["법률", "법", "령", "규칙"],
        words: &["법", "법령", "법률", "시행령", "시행규칙"],
        bare: BareName::Checked,
        weight: Weight::ByLawType,
        missing_note: None,
        scope_line: None,
        writing_note: None,
    },
    NormKind {
        kind: "admrul",
        label: "행정규칙",
        rank: 2,
        tails: &["고시", "훈령", "예규", "지침", "규정"],
        words: &["고시", "훈령", "예규", "지침", "규정"],
        bare: BareName::QuotedOnly,
        weight: Weight::AdminRule,
        missing_note: None,
        scope_line: None,
        writing_note: None,
    },
    NormKind {
        kind: "ordin",
        label: "자치법규",
        rank: 3,
        tails: &["조례", "자치법규"],
        words: &["조례", "자치법규"],
        bare: BareName::RegionOnly,
        weight: Weight::Rule,
        missing_note: Some(NO_ORDINANCE_NOTE),
        scope_line: Some(ORDINANCE_SCOPE_LINE),
        writing_note: Some(ORDINANCE_WRITING_NOTE),
    },
    NormKind {
        kind: "rule",
        label: "기관 규정",
        rank: 4,
        tails: &["학칙", "세칙"],
        words: &["학칙", "세칙", "시행세칙", "내규", "정관", "규약"],
        bare: BareName::Checked,
        weight: Weight::Rule,
        missing_note: None,
        scope_line: None,
        writing_note: Some(RULE_WRITING_NOTE),
    },
];

pub fn kind(name: &str) -> Option<&'static NormKind> {
    KINDS.iter().find(|k| k.kind == name)
}

pub fn label(kind_name: &str) -> &'static str {
    kind(kind_name).map_or("법령", |k| k.label)
}

fn tails_of<'a>(kinds: impl Iterator<Item = &'a NormKind>) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for k in kinds {
        for t in k.tails {
            if !out.contains(t) {
                out.push(t);
            }
        }
    }
    out.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()).then_with(|| a.cmp(b)));
    out
}

pub fn all_tails() -> Vec<&'static str> {
    tails_of(KINDS.iter())
}

pub fn tails_for(bare: BareName) -> Vec<&'static str> {
    tails_of(KINDS.iter().filter(|k| k.bare == bare))
}

pub fn names_a_norm(word: &str) -> bool {
    all_tails().iter().any(|t| word.ends_with(t))
}

pub fn kind_of_name(name: &str) -> Option<&'static NormKind> {
    let order = ["ordin", "rule", "admrul", "law"];
    order.iter().filter_map(|k| kind(k)).find(|k| k.tails.iter().any(|t| name.trim_end().ends_with(t)))
}

pub fn question_words() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for k in &KINDS {
        for w in k.words {
            if !out.contains(w) {
                out.push(w);
            }
        }
    }
    out
}

pub fn kinds_in_question(question: &str) -> Vec<&'static NormKind> {
    KINDS.iter().filter(|k| k.words.iter().any(|w| question.contains(w))).collect()
}

pub fn missing_kinds(question: &str, has_kind: impl Fn(&str) -> bool) -> Vec<&'static NormKind> {
    kinds_in_question(question).into_iter().filter(|k| k.missing_note.is_some() && !has_kind(k.kind)).collect()
}

pub fn is_missing_note(text: &str) -> bool {
    KINDS.iter().any(|k| k.missing_note == Some(text.trim()))
}

pub fn writing_notes<'a>(kinds: impl IntoIterator<Item = &'a str>) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for name in kinds {
        if let Some(note) = kind(name).and_then(|k| k.writing_note)
            && !out.contains(&note)
        {
            out.push(note);
        }
    }
    out
}

pub fn weight_slot(kind_name: &str, law_type: &str) -> Weight {
    match kind(kind_name).map(|k| k.weight) {
        Some(Weight::ByLawType) => match law_type {
            "법률" => Weight::Statute,
            "대통령령" => Weight::Decree,
            _ => Weight::Rule,
        },
        Some(w) => w,
        None => Weight::AdminRule,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tails_are_unique_and_longest_first() {
        let tails = all_tails();
        assert!(tails.contains(&"법") && tails.contains(&"학칙") && tails.contains(&"조례") && tails.contains(&"고시"));
        assert!(tails.iter().position(|t| *t == "법률") < tails.iter().position(|t| *t == "법"));
        assert_eq!(tails.iter().filter(|t| **t == "규정").count(), 1);
        assert!(names_a_norm("단국대학교학칙") && names_a_norm("식품위생법") && names_a_norm("강남구조례") && !names_a_norm("해결방법은"));
        assert_eq!(tails_for(BareName::Checked), vec!["규칙", "법률", "세칙", "학칙", "령", "법"]);
        assert_eq!(tails_for(BareName::RegionOnly), vec!["자치법규", "조례"]);
        assert_eq!(tails_for(BareName::QuotedOnly), vec!["고시", "규정", "예규", "지침", "훈령"]);
    }

    #[test]
    fn kinds_are_told_apart_by_tails_and_words() {
        assert_eq!(kind_of_name("단국대학교 학칙").map(|k| k.kind), Some("rule"));
        assert_eq!(kind_of_name("서울특별시 강남구 식품접객업소 조례").map(|k| k.kind), Some("ordin"));
        assert_eq!(kind_of_name("식품위생법 시행규칙").map(|k| k.kind), Some("law"));
        assert_eq!(kind_of_name("공무원 보수 지침").map(|k| k.kind), Some("admrul"));
        assert_eq!(kind_of_name("김밥").map(|k| k.kind), None);
        assert_eq!(kinds_in_question("학칙에 휴학 규정 있어?").iter().map(|k| k.kind).collect::<Vec<_>>(), vec!["admrul", "rule"]);
        assert!(question_words().contains(&"학칙") && question_words().contains(&"조례") && question_words().contains(&"시행령"));
        assert_eq!(label("rule"), "기관 규정");
        assert_eq!(label("ordin"), "자치법규");
        assert_eq!(label("law"), "법령");
        assert_eq!(label("unknown"), "법령");
    }

    #[test]
    fn missing_notes_and_writing_notes_follow_the_spec() {
        let none = |_: &str| false;
        assert_eq!(missing_kinds("무슨 조례?", none).iter().map(|k| k.kind).collect::<Vec<_>>(), vec!["ordin"]);
        assert!(missing_kinds("무슨 조례?", |k| k == "ordin").is_empty());
        assert!(missing_kinds("무슨 법?", none).is_empty());
        assert!(is_missing_note(NO_ORDINANCE_NOTE) && !is_missing_note("조례가 있습니다."));
        assert_eq!(writing_notes(["rule", "law", "rule"]), vec![RULE_WRITING_NOTE]);
        assert_eq!(writing_notes(["ordin"]), vec![ORDINANCE_WRITING_NOTE]);
        assert!(writing_notes(["law", "admrul"]).is_empty());
    }

    #[test]
    fn weights_follow_kind_then_law_type() {
        assert_eq!(weight_slot("law", "법률"), Weight::Statute);
        assert_eq!(weight_slot("law", "대통령령"), Weight::Decree);
        assert_eq!(weight_slot("law", "부령"), Weight::Rule);
        assert_eq!(weight_slot("rule", "학칙"), Weight::Rule);
        assert_eq!(weight_slot("ordin", "조례"), Weight::Rule);
        assert_eq!(weight_slot("admrul", "고시"), Weight::AdminRule);
        assert_eq!(weight_slot("mystery", ""), Weight::AdminRule);
    }
}
