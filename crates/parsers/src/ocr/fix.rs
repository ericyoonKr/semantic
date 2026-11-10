const WON: char = '원';
const GROUP: usize = 3;
const LEAD_MAX: usize = 3;
const MIN_GROUPS: usize = 2;

fn amount_start(chars: &[char], won: usize) -> Option<usize> {
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
        return (groups >= MIN_GROUPS && lead_ok && !separated).then_some(i);
    }
}

pub(crate) fn amount_separators(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    let wons: Vec<usize> = (0..chars.len()).filter(|&i| chars[i] == WON).collect();
    for won in wons {
        if let Some(start) = amount_start(&chars, won) {
            for c in &mut chars[start..won] {
                if *c == '.' {
                    *c = ',';
                }
            }
        }
    }
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::amount_separators;

    #[test]
    fn periods_in_two_or_more_three_digit_groups_before_won_become_commas() {
        assert_eq!(amount_separators("과징금24.609.019원을"), "과징금24,609,019원을");
        assert_eq!(amount_separators("1,234.567원"), "1,234,567원");
    }

    #[test]
    fn dates_decimals_and_plain_numbers_are_left_alone() {
        assert_eq!(amount_separators("2026.8.10.부터"), "2026.8.10.부터");
        assert_eq!(amount_separators("1.5원"), "1.5원");
        assert_eq!(amount_separators("리터당 123.456원"), "리터당 123.456원");
        assert_eq!(amount_separators("1.25원"), "1.25원");
        assert_eq!(amount_separators("5000원"), "5000원");
        assert_eq!(amount_separators("2026.10.200원"), "2026.10.200원");
    }
}
