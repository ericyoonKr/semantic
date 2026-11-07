use std::sync::LazyLock;

use regex::Regex;

static KOREAN: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^0-9:])(오전|오후)?\s*(영|[0-9]{1,2})시([간작행청점기각효장험설])?(?:\s*([0-9]{1,2})\s*분)?").ok()
});
static COLON: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?:^|[^0-9])([0-9]{1,2}):([0-9]{2})(?:[^0-9]|$)").ok());

fn korean(c: &regex::Captures<'_>) -> Option<(u32, u32)> {
    if c.get(3).is_some() {
        return None;
    }
    let raw = c.get(2)?.as_str();
    let hour: u32 = if raw == "영" { 0 } else { raw.parse().ok()? };
    let minute: u32 = match c.get(4) {
        Some(m) => m.as_str().parse().ok()?,
        None => 0,
    };
    let hour = match c.get(1).map(|m| m.as_str()) {
        Some("오후") if (1..12).contains(&hour) => hour + 12,
        Some("오후") if hour == 12 => 12,
        Some(_) if hour >= 12 => return None,
        _ => hour,
    };
    (hour < 24 && minute < 60).then_some((hour, minute))
}

pub fn clock_times_in(text: &str) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    if let Some(re) = KOREAN.as_ref() {
        out.extend(re.captures_iter(text).filter_map(|c| korean(&c)));
    }
    if let Some(re) = COLON.as_ref() {
        out.extend(re.captures_iter(text).filter_map(|c| {
            let h: u32 = c.get(1)?.as_str().parse().ok()?;
            let m: u32 = c.get(2)?.as_str().parse().ok()?;
            (h < 24 && m < 60).then_some((h, m))
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_korean_and_colon_times_but_not_durations() {
        assert_eq!(clock_times_in("2026. 3. 2. 오전 0시부터"), vec![(0, 0)]);
        assert_eq!(clock_times_in("영시로부터 시작한다"), vec![(0, 0)]);
        assert_eq!(clock_times_in("오후 2시 30분 송달"), vec![(14, 30)]);
        assert_eq!(clock_times_in("10시에 받았다"), vec![(10, 0)]);
        assert_eq!(clock_times_in("00:00 시작, 13:05 도착"), vec![(0, 0), (13, 5)]);
        assert!(clock_times_in("3시간 뒤에 받았다").is_empty());
        assert!(clock_times_in("오전 12시").is_empty());
        assert!(clock_times_in("시행일은 2026. 3. 2.이다").is_empty());
        assert!(clock_times_in("제2시행령에 따른 2 시작").is_empty());
    }
}
