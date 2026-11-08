use std::collections::BTreeMap;

use chrono::NaiveDate;

use super::{PromulNo, UnitKey, Version, VersionStore};
use crate::Error;

#[derive(Debug, Clone)]
struct Promulgation {
    date: NaiveDate,
    default_effective: Option<NaiveDate>,
    unit_effective: BTreeMap<UnitKey, NaiveDate>,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    versions: BTreeMap<UnitKey, Vec<Version>>,
    promulgations: BTreeMap<(String, PromulNo), Promulgation>,
    aliases: BTreeMap<UnitKey, UnitKey>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_version(&mut self, v: Version) -> Result<(), Error> {
        let rows = self.versions.entry(v.unit.clone()).or_default();
        if rows.iter().any(|r| r.valid_period.overlaps(&v.valid_period)) {
            return Err(Error::Overlap(format!("{} {}", v.unit, v.valid_period.from)));
        }
        rows.push(v);
        rows.sort_by_key(|r| r.valid_period.from);
        Ok(())
    }

    pub fn add_promulgation(
        &mut self,
        law_id: &str,
        promul_no: PromulNo,
        date: NaiveDate,
        default_effective: Option<NaiveDate>,
    ) {
        self.promulgations.insert(
            (law_id.to_string(), promul_no),
            Promulgation { date, default_effective, unit_effective: BTreeMap::new() },
        );
    }

    pub fn set_unit_effective(
        &mut self,
        unit: &UnitKey,
        promul_no: &PromulNo,
        effective: NaiveDate,
    ) -> Result<(), Error> {
        let key = (unit.law_id.clone(), promul_no.clone());
        let p = self
            .promulgations
            .get_mut(&key)
            .ok_or_else(|| Error::Table(format!("공포가 등록되지 않음: {promul_no}")))?;
        p.unit_effective.insert(unit.clone(), effective);
        Ok(())
    }

    pub fn add_alias(&mut self, old: UnitKey, current: UnitKey) {
        self.aliases.insert(old, current);
    }

    fn rows(&self, unit: &UnitKey) -> &[Version] {
        self.versions.get(unit).map_or(&[], Vec::as_slice)
    }
}

impl VersionStore for MemoryStore {
    fn at(&self, unit: &UnitKey, date: NaiveDate) -> Result<Option<Version>, Error> {
        let hits: Vec<&Version> =
            self.rows(unit).iter().filter(|v| v.valid_period.contains(date)).collect();
        match hits.as_slice() {
            [] => Ok(None),
            [v] => Ok(Some((*v).clone())),
            _ => Err(Error::Ambiguous(format!("{unit} {date}"))),
        }
    }

    fn between(
        &self,
        unit: &UnitKey,
        from: NaiveDate,
        to: Option<NaiveDate>,
    ) -> Result<Vec<Version>, Error> {
        Ok(self
            .rows(unit)
            .iter()
            .filter(|v| v.valid_period.meets_closed(from, to))
            .cloned()
            .collect())
    }

    fn by_promul(&self, unit: &UnitKey, promul_no: &PromulNo) -> Result<Option<Version>, Error> {
        let hits: Vec<&Version> =
            self.rows(unit).iter().filter(|v| &v.promul_no == promul_no).collect();
        match hits.as_slice() {
            [] => Ok(None),
            [v] => Ok(Some((*v).clone())),
            _ => Err(Error::Ambiguous(format!("{unit} {promul_no}"))),
        }
    }

    fn effective_date(&self, unit: &UnitKey, promul_no: &PromulNo) -> Result<Option<NaiveDate>, Error> {
        if let Some(v) = self.by_promul(unit, promul_no)? {
            return Ok(Some(v.valid_period.from));
        }
        let key = (unit.law_id.clone(), promul_no.clone());
        Ok(self
            .promulgations
            .get(&key)
            .and_then(|p| p.unit_effective.get(unit).copied().or(p.default_effective)))
    }

    fn promul_date(&self, law_id: &str, promul_no: &PromulNo) -> Result<Option<NaiveDate>, Error> {
        let key = (law_id.to_string(), promul_no.clone());
        if let Some(p) = self.promulgations.get(&key) {
            return Ok(Some(p.date));
        }
        Ok(self
            .versions
            .iter()
            .filter(|(u, _)| u.law_id == law_id)
            .flat_map(|(_, rows)| rows.iter())
            .find(|v| &v.promul_no == promul_no)
            .map(|v| v.promul_date))
    }

    fn current_key(&self, unit: &UnitKey) -> Result<Option<UnitKey>, Error> {
        Ok(self.aliases.get(unit).cloned())
    }
}
