const WON: char = '원';
const GROUP: usize = 3;
const LEAD_MAX: usize = 3;
const LONG_DIGIT_RUN: usize = 9;
const YEAR_PREFIXES: [&str; 2] = ["19", "20"];

fn broken_amount(chars: &[char], won: usize) -> bool {
    let mut start = won;
    while start > 0 && (chars[start - 1].is_ascii_digit() || chars[start - 1] == ',') {
        start -= 1;
    }
    let number: String = chars[start..won].iter().collect();
    let number = number.trim_start_matches(',');
    if !number.contains(',') {
        return false;
    }
    let groups: Vec<&str> = number.split(',').collect();
    let lead = groups.first().map_or(0, |g| g.len());
    !(1..=LEAD_MAX).contains(&lead) || groups.iter().skip(1).any(|g| g.len() != GROUP)
}

fn broken_case_number(run: &str) -> bool {
    run.len() >= LONG_DIGIT_RUN && YEAR_PREFIXES.iter().any(|p| run.starts_with(p))
}

pub(crate) fn suspicious(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let amounts = (0..chars.len()).filter(|&i| chars[i] == WON && broken_amount(&chars, i)).count();
    let numbers = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|run| broken_case_number(run))
        .count();
    amounts + numbers
}

#[cfg(test)]
mod tests {
    use super::suspicious;

    #[test]
    fn amount_groups_that_are_not_three_digits_are_suspicious() {
        assert_eq!(suspicious("과징금95,46,883원을"), 1);
        assert_eq!(suspicious("과징금 95,462,883원을"), 0);
        assert_eq!(suspicious("수수료는 3,967원이다"), 0);
        assert_eq!(suspicious("1234,567원"), 1);
        assert_eq!(suspicious("5000원"), 0);
    }

    #[test]
    fn a_year_followed_by_digits_without_a_case_code_is_suspicious() {
        assert_eq!(suspicious("사건번호2025137425관련"), 1);
        assert_eq!(suspicious("사건번호2025두137425관련"), 0);
        assert_eq!(suspicious("2026. 8. 10. 처분"), 0);
        assert_eq!(suspicious("전화 123456789"), 0);
    }
}
