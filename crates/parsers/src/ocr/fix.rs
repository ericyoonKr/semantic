const WON: char = '원';
const GROUP: usize = 3;
const LEAD_MAX: usize = 3;
const PER_UNIT: char = '/';
const MISREAD_HEON: char = '현';
const HEON: char = '헌';
const CONSTITUTIONAL_CODES: [char; 7] = ['마', '바', '가', '나', '라', '사', '아'];
const OPENERS: [char; 2] = ['[', '「'];
const CLOSERS: [char; 2] = [']', '」'];
const LAW_ENDINGS: [&str; 7] = ["법", "법률", "령", "규칙", "조례", "규정", "고시"];
const NOT_LAW_STARTS: [&str; 3] = ["별표", "별지", "서식"];
const MAX_LAW_NAME: usize = 60;

pub(crate) fn correct(text: &str) -> String {
    law_brackets(&case_codes(&amount_separators(text)))
}

struct Amount {
    start: usize,
    groups: usize,
    lead_zero: bool,
}

fn amount_before(chars: &[char], won: usize) -> Option<Amount> {
    let mut i = won;
    let mut groups = 0usize;
    loop {
        let end = i;
        while i > 0 && chars[i - 1].is_ascii_digit() {
            i -= 1;
        }
        let digits = end - i;
        let separated = i > 0 && matches!(chars[i - 1], '.' | ',');
        let before_sep = i >= 2 && chars[i - 2].is_ascii_digit();
        if separated && before_sep && digits == GROUP {
            groups += 1;
            i -= 1;
            continue;
        }
        let lead_ok = (1..=LEAD_MAX).contains(&digits);
        let lead_zero = digits == 1 && chars[i] == '0';
        return (groups > 0 && lead_ok && !separated).then_some(Amount { start: i, groups, lead_zero });
    }
}

fn thousands(amount: &Amount, next: Option<char>) -> bool {
    amount.groups >= 2 || !(amount.lead_zero || next == Some(PER_UNIT))
}

pub(crate) fn amount_separators(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    let wons: Vec<usize> = (0..chars.len()).filter(|&i| chars[i] == WON).collect();
    for won in wons {
        let Some(amount) = amount_before(&chars, won) else { continue };
        if !thousands(&amount, chars.get(won + 1).copied()) {
            continue;
        }
        for c in &mut chars[amount.start..won] {
            if *c == '.' {
                *c = ',';
            }
        }
    }
    chars.into_iter().collect()
}

pub(crate) fn case_codes(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    for i in 1..chars.len().saturating_sub(2) {
        let misread = chars[i] == MISREAD_HEON
            && chars[i - 1].is_ascii_digit()
            && CONSTITUTIONAL_CODES.contains(&chars[i + 1])
            && chars[i + 2].is_ascii_digit();
        if misread {
            chars[i] = HEON;
        }
    }
    chars.into_iter().collect()
}

fn law_name(inner: &str) -> bool {
    let inner = inner.trim();
    !inner.is_empty()
        && inner.chars().count() <= MAX_LAW_NAME
        && LAW_ENDINGS.iter().any(|e| inner.ends_with(e))
        && !NOT_LAW_STARTS.iter().any(|s| inner.starts_with(s))
        && !inner.chars().any(|c| OPENERS.contains(&c) || CLOSERS.contains(&c))
}

pub(crate) fn law_brackets(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = chars.clone();
    let mut open: Option<usize> = None;
    for (i, c) in chars.iter().enumerate() {
        if OPENERS.contains(c) {
            open = Some(i);
        } else if CLOSERS.contains(c)
            && let Some(o) = open.take()
        {
            let inner: String = chars[o + 1..i].iter().collect();
            if law_name(&inner) {
                out[o] = '「';
                out[i] = '」';
            }
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::{amount_separators, case_codes, correct, law_brackets};

    #[test]
    fn periods_in_thousands_groups_before_won_become_commas() {
        assert_eq!(amount_separators("과징금24.609.019원을"), "과징금24,609,019원을");
        assert_eq!(amount_separators("1,234.567원"), "1,234,567원");
        assert_eq!(amount_separators("수수료 366.000원"), "수수료 366,000원");
        assert_eq!(amount_separators("1명당 5.000원"), "1명당 5,000원");
    }

    #[test]
    fn per_unit_decimals_dates_and_plain_numbers_are_left_alone() {
        assert_eq!(amount_separators("킬로그램당 0.027원으로"), "킬로그램당 0.027원으로");
        assert_eq!(amount_separators("(20.605원/㎏)"), "(20.605원/㎏)");
        assert_eq!(amount_separators("2026.8.10.부터"), "2026.8.10.부터");
        assert_eq!(amount_separators("1.5원"), "1.5원");
        assert_eq!(amount_separators("1.25원"), "1.25원");
        assert_eq!(amount_separators("5000원"), "5000원");
        assert_eq!(amount_separators("2026.10.200원"), "2026.10.200원");
    }

    #[test]
    fn heon_is_restored_only_inside_constitutional_case_numbers() {
        assert_eq!(case_codes("2018현마195879"), "2018헌마195879");
        assert_eq!(case_codes("2020현바12"), "2020헌바12");
        assert_eq!(case_codes("현마을"), "현마을");
        assert_eq!(case_codes("2020현재"), "2020현재");
    }

    #[test]
    fn square_brackets_around_law_names_become_corner_brackets() {
        assert_eq!(law_brackets("[식품위생법] 제75조"), "「식품위생법」 제75조");
        assert_eq!(law_brackets("[정부조직법」 및"), "「정부조직법」 및");
        assert_eq!(law_brackets("[국가를 당사자로 하는 계약에 관한 법률]"), "「국가를 당사자로 하는 계약에 관한 법률」");
        assert_eq!(law_brackets("시행규칙 제89조 [별표 23]에 따라"), "시행규칙 제89조 [별표 23]에 따라");
        assert_eq!(law_brackets("[참고]"), "[참고]");
    }

    #[test]
    fn correct_applies_every_rule() {
        assert_eq!(correct("[식품위생법] 2018현마1 과징금 1.000.000원"), "「식품위생법」 2018헌마1 과징금 1,000,000원");
    }
}
