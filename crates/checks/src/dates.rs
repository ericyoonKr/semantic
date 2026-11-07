use std::sync::LazyLock;

use chrono::{Datelike, NaiveDate};
use regex::Regex;

static FULL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*([0-9]{4})\s*[.\-/년]\s*([0-9]{1,2})\s*[.\-/월]\s*([0-9]{1,2})\s*[.일]?\s*$").expect("FULL 정규식")
});
static SHORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*([0-9]{2})\s*[.\-/]\s*([0-9]{1,2})\s*[.\-/]\s*([0-9]{1,2})\s*\.?\s*$").expect("SHORT 정규식")
});

pub fn to_ordinal(d: NaiveDate) -> i64 {
    i64::from(d.num_days_from_ce())
}

pub fn parse_korean_date(s: &str) -> Option<NaiveDate> {
    let c = FULL.captures(s)?;
    let y = c.get(1)?.as_str().parse().ok()?;
    let m = c.get(2)?.as_str().parse().ok()?;
    let d = c.get(3)?.as_str().parse().ok()?;
    NaiveDate::from_ymd_opt(y, m, d)
}

pub fn parse_short_year_candidates(s: &str) -> Vec<NaiveDate> {
    let Some(c) = SHORT.captures(s) else {
        return Vec::new();
    };
    let parts: Option<(i32, u32, u32)> = (|| {
        Some((
            c.get(1)?.as_str().parse().ok()?,
            c.get(2)?.as_str().parse().ok()?,
            c.get(3)?.as_str().parse().ok()?,
        ))
    })();
    let Some((yy, m, d)) = parts else {
        return Vec::new();
    };
    [1900 + yy, 2000 + yy]
        .into_iter()
        .filter_map(|y| NaiveDate::from_ymd_opt(y, m, d))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_common_korean_forms() {
        let want = NaiveDate::from_ymd_opt(2024, 3, 22);
        for s in ["2024. 3. 22.", "2024.3.22", "2024년 3월 22일", "2024-03-22", " 2024 . 03 . 22 "] {
            assert_eq!(parse_korean_date(s), want, "{s}");
        }
    }

    #[test]
    fn rejects_impossible_and_short_years() {
        assert_eq!(parse_korean_date("2023. 2. 29."), None);
        assert_eq!(parse_korean_date("24. 3. 22."), None);
        assert_eq!(parse_korean_date("2024. 13. 1."), None);
    }

    #[test]
    fn short_year_gives_both_centuries() {
        let got = parse_short_year_candidates("24. 3. 22.");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].year(), 1924);
        assert_eq!(got[1].year(), 2024);
    }

    #[test]
    fn ordinal_difference_is_day_count() {
        let a = NaiveDate::from_ymd_opt(2024, 2, 28);
        let b = NaiveDate::from_ymd_opt(2024, 3, 1);
        if let (Some(a), Some(b)) = (a, b) {
            assert_eq!(to_ordinal(b) - to_ordinal(a), 2);
        } else {
            unreachable!();
        }
    }
}
