use chrono::NaiveDate;
use serde::Serialize;

use super::Family;
use crate::Error;
use crate::period::ymd;
use crate::principles::Graph;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Doctrine {
    Motive,
    Text,
    Statute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdjustRule {
    pub id: &'static str,
    pub family: Family,
    pub label: &'static str,
    pub doctrine: Doctrine,
    pub valid_from: Option<NaiveDate>,
    pub valid_until: Option<NaiveDate>,
    pub change_from: Option<NaiveDate>,
    pub refs: &'static [&'static str],
}

impl AdjustRule {
    pub fn valid_on(&self, d: NaiveDate) -> bool {
        self.valid_from.is_none_or(|f| f <= d) && self.valid_until.is_none_or(|u| d < u)
    }
}

pub const EXTRA_REFS: [&str; 3] = ["X:면소", "X:확정후집행면제", "X:과태료확정후면제"];

fn rule(
    id: &'static str,
    family: Family,
    label: &'static str,
    doctrine: Doctrine,
    valid_from: Option<NaiveDate>,
    valid_until: Option<NaiveDate>,
    refs: &'static [&'static str],
) -> AdjustRule {
    AdjustRule { id, family, label, doctrine, valid_from, valid_until, change_from: None, refs }
}

#[derive(Debug, Clone)]
pub struct AdjustTable {
    rules: Vec<AdjustRule>,
}

impl AdjustTable {
    pub fn new(rules: Vec<AdjustRule>) -> Self {
        Self { rules }
    }

    pub fn standard() -> Result<Self, Error> {
        let full_bench = ymd(2022, 12, 22)?;
        let admin_basic = ymd(2021, 3, 23)?;
        Ok(Self::new(vec![
            rule(
                "ADJ-형사-경한신법-동기설",
                Family::CriminalLighter,
                "범죄 후 법령 변경 — 반성적 고려에 따른 변경일 때만 신법(동기설)",
                Doctrine::Motive,
                None,
                Some(full_bench),
                &["X:경한신법", "P:행위시법"],
            ),
            rule(
                "ADJ-형사-경한신법-문언설",
                Family::CriminalLighter,
                "범죄 후 법령 변경 — 변경 동기를 묻지 않고 신법, 유효기간 만료는 변경 아님",
                Doctrine::Text,
                Some(full_bench),
                None,
                &["X:경한신법", "X:동기설폐기", "P:행위시법"],
            ),
            AdjustRule {
                change_from: Some(admin_basic),
                ..rule(
                    "ADJ-행정-제재경한신법",
                    Family::SanctionLighter,
                    "위반행위 후 법령 변경으로 위반이 아니게 되거나 제재가 가벼워지면 변경된 법령",
                    Doctrine::Statute,
                    Some(admin_basic),
                    None,
                    &["X:제재경한신법", "P:제재행위시법"],
                )
            },
            rule(
                "ADJ-행정-과태료유리한신법",
                Family::FineLighter,
                "질서위반행위 후 법률 변경으로 위반이 아니게 되거나 과태료가 가벼워지면 변경된 법률",
                Doctrine::Statute,
                None,
                None,
                &["X:과태료유리한신법", "P:과태료행위시법"],
            ),
            rule(
                "ADJ-행정-신청후처분전개정",
                Family::ApplicationChange,
                "신청 후 처분 전 개정 — 처분시 법령이 원칙, 정당한 이유 없는 지연이면 예외",
                Doctrine::Statute,
                None,
                None,
                &["P:처분시법"],
            ),
            rule(
                "ADJ-조세-과세기간중개정",
                Family::TaxPeriodChange,
                "과세기간 중 개정 — 부진정소급으로 원칙 허용, 신뢰보호와 견줌",
                Doctrine::Statute,
                None,
                None,
                &["P:부진정소급허용", "P:소급과세금지"],
            ),
        ]))
    }

    pub fn rules(&self) -> &[AdjustRule] {
        &self.rules
    }

    pub fn pick(&self, family: Family, as_of: NaiveDate) -> Result<Option<&AdjustRule>, Error> {
        let hits: Vec<&AdjustRule> =
            self.rules.iter().filter(|r| r.family == family && r.valid_on(as_of)).collect();
        match hits.as_slice() {
            [] => Ok(None),
            [r] => Ok(Some(*r)),
            _ => Err(Error::Table(format!("{family:?} 보정 규칙이 {as_of}에 둘 이상 유효함"))),
        }
    }

    pub fn check(&self, graph: &Graph) -> Result<(), Error> {
        for id in self.rules.iter().flat_map(|r| r.refs.iter()).chain(EXTRA_REFS.iter()) {
            graph.rule(id)?;
        }
        Ok(())
    }
}
