const GANADA: [char; 14] = [
    '가', '나', '다', '라', '마', '바', '사', '아', '자', '차', '카', '타', '파', '하',
];
const CHOSUNG: [char; 14] = [
    'ㄱ', 'ㄴ', 'ㄷ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅅ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
];
const ROMAN: [(i64, &str); 13] = [
    (1000, "M"),
    (900, "CM"),
    (500, "D"),
    (400, "CD"),
    (100, "C"),
    (90, "XC"),
    (50, "L"),
    (40, "XL"),
    (10, "X"),
    (9, "IX"),
    (5, "V"),
    (4, "IV"),
    (1, "I"),
];

pub(super) fn format_number(n: i64, fmt: &str) -> Option<String> {
    Some(match fmt {
        "decimal" => n.to_string(),
        "decimalZero" if (0..10).contains(&n) => format!("0{n}"),
        "decimalZero" => n.to_string(),
        "upperRoman" => roman(n),
        "lowerRoman" => roman(n).to_lowercase(),
        "upperLetter" => letter(n, b'A'),
        "lowerLetter" => letter(n, b'a'),
        "ganada" => cycle(n, &GANADA),
        "chosung" => cycle(n, &CHOSUNG),
        "decimalEnclosedCircle" | "decimalEnclosedCircleChinese" => enclosed(n, '\u{2460}'),
        "decimalEnclosedParen" => enclosed(n, '\u{2474}'),
        "decimalEnclosedFullstop" => enclosed(n, '\u{2488}'),
        "decimalFullWidth" | "decimalFullWidth2" => full_width(n),
        "none" => String::new(),
        _ => return None,
    })
}

fn roman(n: i64) -> String {
    if !(1..4000).contains(&n) {
        return n.to_string();
    }
    let mut rest = n;
    let mut out = String::new();
    for (value, symbol) in ROMAN {
        while rest >= value {
            out.push_str(symbol);
            rest -= value;
        }
    }
    out
}

fn letter(n: i64, base: u8) -> String {
    if n < 1 {
        return n.to_string();
    }
    let index = ((n - 1) % 26) as u8;
    let repeat = ((n - 1) / 26 + 1) as usize;
    char::from(base + index).to_string().repeat(repeat)
}

fn cycle(n: i64, table: &[char; 14]) -> String {
    if n < 1 {
        return n.to_string();
    }
    table[((n - 1) % 14) as usize].to_string()
}

fn enclosed(n: i64, first: char) -> String {
    if !(1..=20).contains(&n) {
        return n.to_string();
    }
    char::from_u32(first as u32 + (n - 1) as u32).map_or_else(|| n.to_string(), String::from)
}

fn full_width(n: i64) -> String {
    n.to_string()
        .chars()
        .map(|c| match c.to_digit(10) {
            Some(d) => char::from_u32('\u{FF10}' as u32 + d).unwrap_or(c),
            None => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::format_number;

    #[test]
    fn formats_known_kinds() {
        assert_eq!(format_number(3, "decimal").as_deref(), Some("3"));
        assert_eq!(format_number(3, "ganada").as_deref(), Some("다"));
        assert_eq!(format_number(15, "ganada").as_deref(), Some("가"));
        assert_eq!(format_number(2, "chosung").as_deref(), Some("ㄴ"));
        assert_eq!(format_number(2, "decimalEnclosedCircle").as_deref(), Some("②"));
        assert_eq!(format_number(21, "decimalEnclosedCircle").as_deref(), Some("21"));
        assert_eq!(format_number(14, "upperRoman").as_deref(), Some("XIV"));
        assert_eq!(format_number(4, "lowerRoman").as_deref(), Some("iv"));
        assert_eq!(format_number(28, "upperLetter").as_deref(), Some("BB"));
        assert_eq!(format_number(5, "koreanCounting"), None);
    }
}
