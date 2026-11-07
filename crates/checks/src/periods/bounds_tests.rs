use super::*;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap_or(NaiveDate::MIN)
}

fn covered_2026(extra: &[NaiveDate]) -> Holidays {
    Holidays::with_years(extra.iter().copied(), [2026])
}

#[test]
fn undetermined_start_decides_only_when_both_cases_agree() {
    let h = covered_2026(&[]);
    let b = bounds(d(2026, 3, 2), 90, PeriodUnit::Days, StartKnown::Undetermined, &h);
    let Some(b) = b else {
        unreachable!();
    };
    assert_eq!(b.cases.len(), 2);
    assert_eq!(b.cases[0].1.end, d(2026, 5, 31));
    assert_eq!(b.cases[1].1.end, d(2026, 5, 30));
    assert_eq!((b.lo, b.hi), (d(2026, 6, 1), Some(d(2026, 6, 1))));
    assert_eq!(b.within(d(2026, 6, 1)), Some(true));
    assert_eq!(b.within(d(2026, 6, 2)), Some(false));
}

#[test]
fn undetermined_start_on_the_boundary_day_is_left_open() {
    let h = covered_2026(&[]);
    let b = bounds(d(2026, 1, 9), 90, PeriodUnit::Days, StartKnown::Undetermined, &h);
    let Some(b) = b else {
        unreachable!();
    };
    assert_eq!((b.lo, b.hi), (d(2026, 4, 8), Some(d(2026, 4, 9))));
    assert_eq!(b.within(d(2026, 4, 8)), Some(true));
    assert_eq!(b.within(d(2026, 4, 9)), None);
    assert_eq!(b.within(d(2026, 4, 10)), Some(false));
}

#[test]
fn lunar_holiday_from_data_moves_the_last_day_and_rechecks_the_moved_day() {
    let seollal = [d(2026, 2, 16), d(2026, 2, 17), d(2026, 2, 18)];
    let h = covered_2026(&seollal);
    let b = bounds(d(2025, 11, 17), 90, PeriodUnit::Days, StartKnown::Known(Start::AfterEvent), &Holidays::with_years(seollal, [2026, 2025]));
    let Some(b) = b else {
        unreachable!();
    };
    assert_eq!(b.cases[0].1.end, d(2026, 2, 15));
    assert_eq!(b.cases[0].1.skipped, vec![d(2026, 2, 15), d(2026, 2, 16), d(2026, 2, 17), d(2026, 2, 18)]);
    assert_eq!((b.lo, b.hi), (d(2026, 2, 19), Some(d(2026, 2, 19))));
    assert!(h.is_closed(d(2026, 2, 17)));
}

#[test]
fn saturday_and_fixed_holiday_move_with_known_start() {
    let b = bounds(d(2026, 2, 26), 2, PeriodUnit::Days, StartKnown::Known(Start::AfterEvent), &Holidays::default());
    let Some(b) = b else {
        unreachable!();
    };
    assert_eq!(b.cases[0].1.skipped, vec![d(2026, 2, 28), d(2026, 3, 1)]);
    assert_eq!((b.lo, b.hi), (d(2026, 3, 2), None));
}

#[test]
fn year_without_holiday_data_gives_only_a_lower_bound() {
    let none = Holidays::default();
    let b = bounds(d(2026, 3, 2), 90, PeriodUnit::Days, StartKnown::Known(Start::AfterEvent), &none);
    let Some(b) = b else {
        unreachable!();
    };
    assert_eq!(b.cases[0].1.uncovered, BTreeSet::from([2026]));
    assert_eq!((b.lo, b.hi), (d(2026, 6, 1), None));
    assert_eq!(b.within(d(2026, 5, 20)), Some(true));
    assert_eq!(b.within(d(2026, 7, 1)), None);
}

#[test]
fn one_year_period_counts_by_calendar() {
    let h = Holidays::with_years([], [2027]);
    let b = bounds(d(2026, 3, 2), 1, PeriodUnit::Years, StartKnown::Known(Start::AfterEvent), &h);
    let Some(b) = b else {
        unreachable!();
    };
    assert_eq!(b.cases[0].1.end, d(2027, 3, 2));
    assert_eq!(b.lo, d(2027, 3, 2));
}
