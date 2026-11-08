use chrono::{Days, NaiveDate};
use serde::Serialize;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Period {
    pub from: NaiveDate,
    pub until: Option<NaiveDate>,
}

impl Period {
    pub fn new(from: NaiveDate, until: Option<NaiveDate>) -> Result<Self, Error> {
        match until {
            Some(u) if u <= from => Err(Error::BadPeriod),
            _ => Ok(Self { from, until }),
        }
    }

    pub fn contains(&self, d: NaiveDate) -> bool {
        d >= self.from && self.until.is_none_or(|u| d < u)
    }

    pub fn overlaps(&self, other: &Period) -> bool {
        let a = self.until.is_none_or(|u| other.from < u);
        let b = other.until.is_none_or(|u| self.from < u);
        a && b
    }

    pub fn meets_closed(&self, from: NaiveDate, to: Option<NaiveDate>) -> bool {
        let starts_before_end = to.is_none_or(|t| self.from <= t);
        let ends_after_start = self.until.is_none_or(|u| from < u);
        starts_before_end && ends_after_start
    }
}

pub fn prev_day(d: NaiveDate) -> Result<NaiveDate, Error> {
    d.checked_sub_days(Days::new(1)).ok_or(Error::DateRange)
}

pub fn next_day(d: NaiveDate) -> Result<NaiveDate, Error> {
    d.checked_add_days(Days::new(1)).ok_or(Error::DateRange)
}

pub fn ymd(y: i32, m: u32, d: u32) -> Result<NaiveDate, Error> {
    NaiveDate::from_ymd_opt(y, m, d).ok_or(Error::DateRange)
}
