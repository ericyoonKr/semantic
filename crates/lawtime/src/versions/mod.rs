mod memory;

use std::fmt;

use chrono::NaiveDate;
use serde::Serialize;

pub use memory::MemoryStore;

use crate::Error;
use crate::period::Period;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum UnitKind {
    Article,
    Annex,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct UnitKey {
    pub law_id: String,
    pub kind: UnitKind,
    pub key: String,
}

impl UnitKey {
    pub fn article(law_id: &str, key: &str) -> Self {
        Self { law_id: law_id.to_string(), kind: UnitKind::Article, key: key.to_string() }
    }

    pub fn annex(law_id: &str, key: &str) -> Self {
        Self { law_id: law_id.to_string(), kind: UnitKind::Annex, key: key.to_string() }
    }
}

impl fmt::Display for UnitKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            UnitKind::Article => "조문",
            UnitKind::Annex => "별표",
        };
        write!(f, "{}/{kind}/{}", self.law_id, self.key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct PromulNo {
    pub issuer: String,
    pub number: u32,
}

impl PromulNo {
    pub fn new(issuer: &str, number: u32) -> Self {
        Self { issuer: issuer.to_string(), number }
    }

    pub fn statute(number: u32) -> Self {
        Self::new("법률", number)
    }

    pub fn is_statute(&self) -> bool {
        self.issuer == "법률"
    }
}

impl fmt::Display for PromulNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} 제{}호", self.issuer, self.number)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EndReason {
    Amended,
    Repealed,
    Expired,
    Unconstitutional,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Version {
    pub unit: UnitKey,
    pub promul_no: PromulNo,
    pub promul_date: NaiveDate,
    pub valid_period: Period,
    pub end_reason: Option<EndReason>,
    pub body: String,
}

impl Version {
    pub fn law_id(&self) -> &str {
        &self.unit.law_id
    }

    pub fn same_row(&self, other: &Version) -> bool {
        self.unit == other.unit && self.valid_period.from == other.valid_period.from
    }
}

pub trait VersionStore {
    fn at(&self, unit: &UnitKey, date: NaiveDate) -> Result<Option<Version>, Error>;
    fn between(
        &self,
        unit: &UnitKey,
        from: NaiveDate,
        to: Option<NaiveDate>,
    ) -> Result<Vec<Version>, Error>;
    fn by_promul(&self, unit: &UnitKey, promul_no: &PromulNo) -> Result<Option<Version>, Error>;
    fn effective_date(&self, unit: &UnitKey, promul_no: &PromulNo) -> Result<Option<NaiveDate>, Error>;
    fn promul_date(&self, law_id: &str, promul_no: &PromulNo) -> Result<Option<NaiveDate>, Error>;
    fn current_key(&self, unit: &UnitKey) -> Result<Option<UnitKey>, Error>;
}
