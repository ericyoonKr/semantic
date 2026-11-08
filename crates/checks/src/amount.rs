use std::sync::LazyLock;

use regex::Regex;

static DIGITS: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^[0-9][0-9,]*(\.[0-9]+)?$").ok());

const SMALL: [(char, i64); 3] = [('십', 10), ('백', 100), ('천', 1000)];
const LARGE: [(char, i64); 3] = [('만', 10_000), ('억', 100_000_000), ('조', 1_000_000_000_000)];
const HANGUL_DIGITS: [(char, i64); 9] = [('일', 1), ('이', 2), ('삼', 3), ('사', 4), ('오', 5), ('육', 6), ('칠', 7), ('팔', 8), ('구', 9)];

fn strip(s: &str) -> String {
    let mut t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if let Some(rest) = t.strip_prefix('금') {
        t = rest.to_string();
    }
    for tail in ["원정", "원", "정"] {
        if let Some(rest) = t.strip_suffix(tail) {
            t = rest.to_string();
        }
    }
    t
}

fn plain_number(t: &str) -> Option<i64> {
    if !DIGITS.as_ref().is_some_and(|r| r.is_match(t)) || t.contains('.') {
        return None;
    }
    t.replace(',', "").parse().ok()
}

fn small_part(t: &str) -> Option<i64> {
    if t.is_empty() {
        return Some(0);
    }
    if let Some(n) = plain_number(t) {
        return Some(n);
    }
    let mut total = 0i64;
    let mut pending: Option<i64> = None;
    let mut digits = String::new();
    for c in t.chars() {
        if c.is_ascii_digit() || c == ',' {
            digits.push(c);
            continue;
        }
        if !digits.is_empty() {
            pending = Some(plain_number(&digits)?);
            digits.clear();
        }
        if let Some((_, v)) = HANGUL_DIGITS.iter().find(|(h, _)| *h == c) {
            pending = Some(*v);
        } else if let Some((_, unit)) = SMALL.iter().find(|(h, _)| *h == c) {
            total = total.checked_add(pending.take().unwrap_or(1).checked_mul(*unit)?)?;
        } else {
            return None;
        }
    }
    if !digits.is_empty() {
        pending = Some(plain_number(&digits)?);
    }
    total.checked_add(pending.unwrap_or(0))
}

pub fn parse_amount(s: &str) -> Option<i64> {
    let t = strip(s);
    if t.is_empty() {
        return None;
    }
    if let Some(n) = plain_number(&t) {
        return Some(n);
    }
    let mut total = 0i64;
    let mut rest = t.as_str();
    for (unit_char, unit) in LARGE.iter().rev() {
        if let Some(pos) = rest.find(*unit_char) {
            let (head, tail) = rest.split_at(pos);
            let head_value = if head.is_empty() { 1 } else { small_part(head)? };
            total = total.checked_add(head_value.checked_mul(*unit)?)?;
            rest = &tail[unit_char.len_utf8()..];
        }
    }
    total.checked_add(small_part(rest)?)
}

pub fn parse_percent(s: &str) -> Option<f64> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let t = t.strip_suffix('%')?;
    t.replace(',', "").parse().ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn patterns_compile() {
        assert!(super::DIGITS.is_some());
    }

    use super::*;

    #[test]
    fn digits_and_units() {
        assert_eq!(parse_amount("1,234,000원"), Some(1_234_000));
        assert_eq!(parse_amount("금 오천만원"), Some(50_000_000));
        assert_eq!(parse_amount("5천만 원"), Some(50_000_000));
        assert_eq!(parse_amount("12억 3,400만원"), Some(1_234_000_000));
        assert_eq!(parse_amount("금 일억이천삼백만원정"), Some(123_000_000));
        assert_eq!(parse_amount("삼십만 원"), Some(300_000));
    }

    #[test]
    fn non_amounts_are_none() {
        assert_eq!(parse_amount("합계"), None);
        assert_eq!(parse_amount("1.5%"), None);
        assert_eq!(parse_amount(""), None);
        assert_eq!(parse_percent("12.5 %"), Some(12.5));
    }
}
