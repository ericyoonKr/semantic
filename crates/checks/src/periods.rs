use std::collections::BTreeSet;

use chrono::{Datelike, Days, Months, NaiveDate, Weekday};

const FIXED_SOLAR: [(u32, u32); 8] = [(1, 1), (3, 1), (5, 5), (6, 6), (8, 15), (10, 3), (10, 9), (12, 25)];

#[derive(Debug, Clone, Default)]
pub struct Holidays {
    extra: BTreeSet<NaiveDate>,
}

impl Holidays {
    pub fn new(extra: impl IntoIterator<Item = NaiveDate>) -> Self {
        Self { extra: extra.into_iter().collect() }
    }

    pub fn is_closed(&self, d: NaiveDate) -> bool {
        matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
            || FIXED_SOLAR.contains(&(d.month(), d.day()))
            || self.extra.contains(&d)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    AfterEvent,
    AtMidnight,
}

fn first_day(start: NaiveDate, how: Start) -> Option<NaiveDate> {
    match how {
        Start::AfterEvent => start.checked_add_days(Days::new(1)),
        Start::AtMidnight => Some(start),
    }
}

fn roll_forward(mut d: NaiveDate, holidays: &Holidays) -> Option<NaiveDate> {
    while holidays.is_closed(d) {
        d = d.checked_add_days(Days::new(1))?;
    }
    Some(d)
}

fn last_day_of_month(year: i32, month: u32) -> Option<NaiveDate> {
    let next = if month == 12 { NaiveDate::from_ymd_opt(year + 1, 1, 1)? } else { NaiveDate::from_ymd_opt(year, month + 1, 1)? };
    next.pred_opt()
}

fn calendar_end(first: NaiveDate, months: u32) -> Option<NaiveDate> {
    let shifted_month_first = NaiveDate::from_ymd_opt(first.year(), first.month(), 1)?.checked_add_months(Months::new(months))?;
    match NaiveDate::from_ymd_opt(shifted_month_first.year(), shifted_month_first.month(), first.day()) {
        Some(corresponding) => corresponding.pred_opt(),
        None => last_day_of_month(shifted_month_first.year(), shifted_month_first.month()),
    }
}

pub fn deadline_days(start: NaiveDate, days: u32, how: Start, holidays: &Holidays) -> Option<NaiveDate> {
    if days == 0 {
        return None;
    }
    let end = first_day(start, how)?.checked_add_days(Days::new(u64::from(days) - 1))?;
    roll_forward(end, holidays)
}

pub fn deadline_months(start: NaiveDate, months: u32, how: Start, holidays: &Holidays) -> Option<NaiveDate> {
    if months == 0 {
        return None;
    }
    roll_forward(calendar_end(first_day(start, how)?, months)?, holidays)
}

pub fn deadline_years(start: NaiveDate, years: u32, how: Start, holidays: &Holidays) -> Option<NaiveDate> {
    deadline_months(start, years.checked_mul(12)?, how, holidays)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap_or(NaiveDate::MIN)
    }

    #[test]
    fn ninety_days_excludes_first_day() {
        let none = Holidays::default();
        assert_eq!(deadline_days(d(2025, 1, 10), 90, Start::AfterEvent, &none), Some(d(2025, 4, 10)));
        assert_eq!(deadline_days(d(2025, 1, 10), 90, Start::AtMidnight, &none), Some(d(2025, 4, 9)));
    }

    #[test]
    fn last_day_on_saturday_moves_to_next_open_day() {
        let none = Holidays::default();
        assert_eq!(d(2026, 3, 7).weekday(), Weekday::Sat);
        assert_eq!(deadline_days(d(2026, 2, 5), 30, Start::AfterEvent, &none), Some(d(2026, 3, 9)));
    }

    #[test]
    fn extra_holiday_is_skipped() {
        let lunar = Holidays::new([d(2026, 3, 9)]);
        assert_eq!(deadline_days(d(2026, 2, 5), 30, Start::AfterEvent, &lunar), Some(d(2026, 3, 10)));
    }

    #[test]
    fn one_year_across_leap_day_uses_calendar() {
        let none = Holidays::default();
        assert_eq!(deadline_years(d(2023, 3, 14), 1, Start::AfterEvent, &none), Some(d(2024, 3, 14)));
        assert_eq!(deadline_years(d(2024, 2, 28), 1, Start::AfterEvent, &none), Some(d(2025, 2, 28)));
    }

    #[test]
    fn missing_corresponding_day_ends_on_month_end() {
        let none = Holidays::default();
        assert_eq!(deadline_months(d(2025, 1, 31), 1, Start::AtMidnight, &none), Some(d(2025, 2, 28)));
        assert_eq!(deadline_months(d(2024, 1, 31), 1, Start::AtMidnight, &none), Some(d(2024, 2, 29)));
    }

    #[test]
    fn fixed_solar_holiday_and_sunday_roll_forward() {
        let none = Holidays::default();
        assert_eq!(d(2026, 2, 28).weekday(), Weekday::Sat);
        assert_eq!(deadline_days(d(2026, 2, 26), 2, Start::AfterEvent, &none), Some(d(2026, 3, 2)));
    }
}
