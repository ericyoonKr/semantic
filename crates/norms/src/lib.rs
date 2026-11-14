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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Law,
    AdminRule,
    Ordinance,
    Rule,
}

pub trait Norm {
    fn kind(&self) -> Kind;
    fn label(&self) -> &'static str;
    fn rank(&self) -> u8;
    fn tails(&self) -> &'static [&'static str];
    fn words(&self) -> &'static [&'static str];
    fn bare(&self) -> BareName;
    fn weight(&self) -> Weight;
    fn missing_note(&self) -> Option<&'static str> {
        None
    }
    fn scope_line(&self) -> Option<&'static str> {
        None
    }
    fn writing_note(&self) -> Option<&'static str> {
        None
    }
}

pub const NO_ORDINANCE_NOTE: &str = "조례(자치법규)는 이 서비스의 자료에 없어 확인하지 못했습니다.";
pub const ORDINANCE_SCOPE_LINE: &str = "자료 범위: 자치법규(조례·규칙)는 자료에 없음. 상위 법령(법률·시행령·시행규칙·별표)과 판례에서 찾는다.";
pub const ORDINANCE_WRITING_NOTE: &str =
    "조례·규칙은 지방자치단체의 자치법규다. 법률과 구분해 「조례상 …」으로 적고, 상위 법령과 어긋나는 점이 보이면 그것도 적는다.";
pub const RULE_WRITING_NOTE: &str =
    "학칙·세칙 같은 기관 규정은 법률이 아니라 그 기관 구성원에게만 미치는 자치규범이다. 자료에서 읽은 학칙 조문은 법령 조문과 똑같이 근거로 쓰되 「학칙상 …」·「규정상 …」으로 적어 법률상 근거와 구분하고, 상위 법령(고등교육법 등)이 있으면 둘의 관계를 함께 적는다. 읽은 학칙 조문이 있는데 「학칙을 찾지 못했다」고 쓰지 않는다.";

pub struct Law;
pub struct AdminRule;
pub struct Ordinance;
pub struct Rule;

impl Norm for Law {
    fn kind(&self) -> Kind {
        Kind::Law
    }
    fn label(&self) -> &'static str {
        "법령"
    }
    fn rank(&self) -> u8 {
        1
    }
    fn tails(&self) -> &'static [&'static str] {
        &["법률", "법", "령", "규칙"]
    }
    fn words(&self) -> &'static [&'static str] {
        &["법", "법령", "법률", "시행령", "시행규칙"]
    }
    fn bare(&self) -> BareName {
        BareName::Checked
    }
    fn weight(&self) -> Weight {
        Weight::ByLawType
    }
}

impl Norm for AdminRule {
    fn kind(&self) -> Kind {
        Kind::AdminRule
    }
    fn label(&self) -> &'static str {
        "행정규칙"
    }
    fn rank(&self) -> u8 {
        2
    }
    fn tails(&self) -> &'static [&'static str] {
        &["고시", "훈령", "예규", "지침", "규정"]
    }
    fn words(&self) -> &'static [&'static str] {
        &["고시", "훈령", "예규", "지침", "규정"]
    }
    fn bare(&self) -> BareName {
        BareName::QuotedOnly
    }
    fn weight(&self) -> Weight {
        Weight::AdminRule
    }
}

impl Norm for Ordinance {
    fn kind(&self) -> Kind {
        Kind::Ordinance
    }
    fn label(&self) -> &'static str {
        "자치법규"
    }
    fn rank(&self) -> u8 {
        3
    }
    fn tails(&self) -> &'static [&'static str] {
        &["조례", "자치법규"]
    }
    fn words(&self) -> &'static [&'static str] {
        &["조례", "자치법규"]
    }
    fn bare(&self) -> BareName {
        BareName::RegionOnly
    }
    fn weight(&self) -> Weight {
        Weight::Rule
    }
    fn missing_note(&self) -> Option<&'static str> {
        Some(NO_ORDINANCE_NOTE)
    }
    fn scope_line(&self) -> Option<&'static str> {
        Some(ORDINANCE_SCOPE_LINE)
    }
    fn writing_note(&self) -> Option<&'static str> {
        Some(ORDINANCE_WRITING_NOTE)
    }
}

impl Norm for Rule {
    fn kind(&self) -> Kind {
        Kind::Rule
    }
    fn label(&self) -> &'static str {
        "기관 규정"
    }
    fn rank(&self) -> u8 {
        4
    }
    fn tails(&self) -> &'static [&'static str] {
        &["학칙", "세칙"]
    }
    fn words(&self) -> &'static [&'static str] {
        &["학칙", "세칙", "시행세칙", "내규", "정관", "규약"]
    }
    fn bare(&self) -> BareName {
        BareName::Checked
    }
    fn weight(&self) -> Weight {
        Weight::Rule
    }
    fn writing_note(&self) -> Option<&'static str> {
        Some(RULE_WRITING_NOTE)
    }
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Law, Kind::AdminRule, Kind::Ordinance, Kind::Rule];
    const BY_NAME: [Kind; 4] = [Kind::Ordinance, Kind::Rule, Kind::AdminRule, Kind::Law];

    pub fn norm(self) -> &'static dyn Norm {
        match self {
            Kind::Law => &Law,
            Kind::AdminRule => &AdminRule,
            Kind::Ordinance => &Ordinance,
            Kind::Rule => &Rule,
        }
    }

    pub fn db_value(self) -> &'static str {
        match self {
            Kind::Law => "law",
            Kind::AdminRule => "admrul",
            Kind::Ordinance => "ordin",
            Kind::Rule => "rule",
        }
    }

    pub fn from_db(value: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.db_value() == value.trim())
    }

    pub fn of_name(name: &str) -> Option<Kind> {
        let name = name.trim_end();
        Kind::BY_NAME.into_iter().find(|k| k.norm().tails().iter().any(|t| name.ends_with(t)))
    }
}

pub fn all() -> impl Iterator<Item = &'static dyn Norm> {
    Kind::ALL.into_iter().map(Kind::norm)
}

pub fn label(db_value: &str) -> &'static str {
    Kind::from_db(db_value).map_or(Law.label(), |k| k.norm().label())
}

fn tails_of<'a>(norms: impl Iterator<Item = &'a dyn Norm>) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for n in norms {
        for t in n.tails() {
            if !out.contains(t) {
                out.push(t);
            }
        }
    }
    out.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()).then_with(|| a.cmp(b)));
    out
}

pub fn all_tails() -> Vec<&'static str> {
    tails_of(all())
}

pub fn tails_for(bare: BareName) -> Vec<&'static str> {
    tails_of(all().filter(|n| n.bare() == bare))
}

pub fn names_a_norm(word: &str) -> bool {
    all_tails().iter().any(|t| word.ends_with(t))
}

pub fn question_words() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for n in all() {
        for w in n.words() {
            if !out.contains(w) {
                out.push(w);
            }
        }
    }
    out
}

pub fn kinds_in_question(question: &str) -> Vec<Kind> {
    Kind::ALL.into_iter().filter(|k| k.norm().words().iter().any(|w| question.contains(w))).collect()
}

pub fn missing_kinds(question: &str, has: impl Fn(Kind) -> bool) -> Vec<Kind> {
    kinds_in_question(question).into_iter().filter(|k| k.norm().missing_note().is_some() && !has(*k)).collect()
}

pub fn is_missing_note(text: &str) -> bool {
    all().any(|n| n.missing_note() == Some(text.trim()))
}

pub fn writing_notes(kinds: impl IntoIterator<Item = Kind>) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for k in kinds {
        if let Some(note) = k.norm().writing_note()
            && !out.contains(&note)
        {
            out.push(note);
        }
    }
    out
}

pub fn weight_slot(kind: Kind, law_type: &str) -> Weight {
    match kind.norm().weight() {
        Weight::ByLawType => match law_type {
            "법률" => Weight::Statute,
            "대통령령" => Weight::Decree,
            _ => Weight::Rule,
        },
        w => w,
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
    fn kinds_are_made_from_db_values_and_names() {
        assert_eq!(Kind::of_name("단국대학교 학칙"), Some(Kind::Rule));
        assert_eq!(Kind::of_name("서울특별시 강남구 식품접객업소 조례"), Some(Kind::Ordinance));
        assert_eq!(Kind::of_name("식품위생법 시행규칙"), Some(Kind::Law));
        assert_eq!(Kind::of_name("공무원 보수 지침"), Some(Kind::AdminRule));
        assert_eq!(Kind::of_name("김밥"), None);
        for k in Kind::ALL {
            assert_eq!(Kind::from_db(k.db_value()), Some(k));
            assert_eq!(k.norm().kind(), k);
        }
        assert_eq!(Kind::from_db("mystery"), None);
        assert_eq!(kinds_in_question("학칙에 휴학 규정 있어?"), vec![Kind::AdminRule, Kind::Rule]);
        assert!(question_words().contains(&"학칙") && question_words().contains(&"조례") && question_words().contains(&"시행령"));
        assert_eq!(label("rule"), "기관 규정");
        assert_eq!(label("ordin"), "자치법규");
        assert_eq!(label("law"), "법령");
        assert_eq!(label("unknown"), "법령");
        assert!(all().map(|n| n.rank()).collect::<Vec<_>>().windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn missing_notes_and_writing_notes_follow_the_spec() {
        let none = |_: Kind| false;
        assert_eq!(missing_kinds("무슨 조례?", none), vec![Kind::Ordinance]);
        assert!(missing_kinds("무슨 조례?", |k| k == Kind::Ordinance).is_empty());
        assert!(missing_kinds("무슨 법?", none).is_empty());
        assert!(is_missing_note(NO_ORDINANCE_NOTE) && !is_missing_note("조례가 있습니다."));
        assert_eq!(writing_notes([Kind::Rule, Kind::Law, Kind::Rule]), vec![RULE_WRITING_NOTE]);
        assert_eq!(writing_notes([Kind::Ordinance]), vec![ORDINANCE_WRITING_NOTE]);
        assert!(writing_notes([Kind::Law, Kind::AdminRule]).is_empty());
    }

    #[test]
    fn weights_follow_kind_then_law_type() {
        assert_eq!(weight_slot(Kind::Law, "법률"), Weight::Statute);
        assert_eq!(weight_slot(Kind::Law, "대통령령"), Weight::Decree);
        assert_eq!(weight_slot(Kind::Law, "부령"), Weight::Rule);
        assert_eq!(weight_slot(Kind::Rule, "학칙"), Weight::Rule);
        assert_eq!(weight_slot(Kind::Ordinance, "조례"), Weight::Rule);
        assert_eq!(weight_slot(Kind::AdminRule, "고시"), Weight::AdminRule);
    }
}
