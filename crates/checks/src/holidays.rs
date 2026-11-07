use std::collections::BTreeSet;
use std::path::Path;

use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use thiserror::Error;

use crate::periods::Holidays;

pub const SCHEMA: &str = "holidays-kr/1";

#[derive(Debug, Clone)]
pub enum HolidayData {
    NotSet,
    Loaded(Holidays),
    Failed(String),
}

impl HolidayData {
    pub fn table(&self) -> Holidays {
        match self {
            HolidayData::Loaded(h) => h.clone(),
            HolidayData::NotSet | HolidayData::Failed(_) => Holidays::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HolidayFileError {
    #[error("공휴일 파일을 읽지 못함: {0}")]
    Read(String),
    #[error("공휴일 파일 형식이 맞지 않음: {0}")]
    Parse(String),
    #[error("공휴일 파일의 schema가 {SCHEMA}가 아님: {0}")]
    Schema(String),
    #[error("공휴일 날짜를 읽지 못함: {0}")]
    BadDate(String),
    #[error("{year}년 목록에 다른 해의 날짜가 있음: {date}")]
    WrongYear { year: i32, date: String },
    #[error("같은 해가 두 번 있음: {0}")]
    DuplicateYear(i32),
    #[error("{0}년 목록에 출처가 없음")]
    NoSource(i32),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema: String,
    #[serde(default)]
    basis: String,
    years: Vec<Year>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Year {
    year: i32,
    sources: Vec<String>,
    holidays: Vec<Day>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Day {
    date: String,
    name: String,
}

pub fn parse(text: &str) -> Result<Holidays, HolidayFileError> {
    let file: File = serde_json::from_str(text).map_err(|e| HolidayFileError::Parse(e.to_string()))?;
    if file.schema != SCHEMA {
        return Err(HolidayFileError::Schema(file.schema));
    }
    let _ = file.basis;
    let mut years = BTreeSet::new();
    let mut dates = Vec::new();
    for y in file.years {
        if !years.insert(y.year) {
            return Err(HolidayFileError::DuplicateYear(y.year));
        }
        if y.sources.iter().all(|s| s.trim().is_empty()) {
            return Err(HolidayFileError::NoSource(y.year));
        }
        for day in y.holidays {
            let _ = day.name;
            let date = NaiveDate::parse_from_str(day.date.trim(), "%Y-%m-%d").map_err(|_| HolidayFileError::BadDate(day.date.clone()))?;
            if date.year() != y.year {
                return Err(HolidayFileError::WrongYear { year: y.year, date: day.date });
            }
            dates.push(date);
        }
    }
    Ok(Holidays::with_years(dates, years))
}

pub fn load(path: Option<&Path>) -> HolidayData {
    let Some(path) = path else {
        return HolidayData::NotSet;
    };
    let read = std::fs::read_to_string(path).map_err(|e| HolidayFileError::Read(e.to_string()));
    match read.and_then(|text| parse(&text)) {
        Ok(h) => HolidayData::Loaded(h),
        Err(e) => HolidayData::Failed(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap_or(NaiveDate::MIN)
    }

    const GOOD: &str = r#"{"schema": "holidays-kr/1", "basis": "b", "years": [
        {"year": 2026, "sources": ["s"], "holidays": [{"date": "2026-02-17", "name": "설날"}]}]}"#;

    #[test]
    fn file_gives_dates_and_covered_years() -> Result<(), HolidayFileError> {
        let h = parse(GOOD)?;
        assert!(h.covers(2026) && !h.covers(2025));
        assert!(h.is_closed(d(2026, 2, 17)));
        assert!(!h.is_closed(d(2026, 2, 19)));
        Ok(())
    }

    #[test]
    fn bad_files_are_refused_whole() {
        let wrong_year = GOOD.replace("\"2026-02-17\"", "\"2025-02-17\"");
        assert!(matches!(parse(&wrong_year), Err(HolidayFileError::WrongYear { .. })));
        let no_source = GOOD.replace("[\"s\"]", "[]");
        assert!(matches!(parse(&no_source), Err(HolidayFileError::NoSource(2026))));
        let schema = GOOD.replace("holidays-kr/1", "other/1");
        assert!(matches!(parse(&schema), Err(HolidayFileError::Schema(_))));
        let bad = GOOD.replace("2026-02-17", "2026-02-30");
        assert!(matches!(parse(&bad), Err(HolidayFileError::BadDate(_))));
        assert!(matches!(load(None), HolidayData::NotSet));
        let missing = load(Some(Path::new("no-such-holiday-file.json")));
        assert!(matches!(missing, HolidayData::Failed(_)));
        assert!(!missing.table().covers(2026));
    }
}
