const BOX_CHARS: &[char] = &[
    '│', '─', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼', '┃', '━', '┏', '┓', '┗', '┛', '┣', '┫', '┳', '┻', '╋', '═', '║', '╔', '╗',
    '╚', '╝', '╠', '╣', '╦', '╩', '╬',
];
const CONTINUATION_INDENT: &str = "  ";
const NO_SPACE_BEFORE: &[char] = &[')', ']', '」', '』', ',', '.', '·', ':', ';'];
const NO_SPACE_AFTER: &[char] = &['(', '[', '「', '『'];

fn has_box(line: &str) -> bool {
    line.chars().any(|c| BOX_CHARS.contains(&c))
}

fn is_hangul(c: char) -> bool {
    ('가'..='힣').contains(&c)
}

fn continuation(line: &str) -> Option<&str> {
    let rest = line.strip_prefix(CONTINUATION_INDENT)?;
    let first = rest.chars().next()?;
    (first != ' ' && first != '\t').then_some(rest)
}

fn joiner(prev: &str, next: &str) -> &'static str {
    match (prev.chars().last(), next.chars().next()) {
        (Some(a), Some(b)) if is_hangul(a) && is_hangul(b) => "",
        (Some(a), _) if NO_SPACE_AFTER.contains(&a) => "",
        (_, Some(b)) if NO_SPACE_BEFORE.contains(&b) => "",
        _ => " ",
    }
}

pub fn reflow_wrapped(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.trim_end_matches('\r');
        if let Some(rest) = continuation(line)
            && let Some(prev) = out.last_mut()
            && !prev.trim().is_empty()
            && !has_box(prev)
            && !has_box(line)
        {
            let sep = joiner(prev, rest);
            prev.push_str(sep);
            prev.push_str(rest);
            continue;
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::reflow_wrapped;

    #[test]
    fn wrapped_prose_lines_are_joined_but_items_headings_and_box_tables_are_kept() {
        let text = "■ 식품위생법 시행규칙 [별표 23]\n\n  행정처분 기준(제89조 관련)\n\n  Ⅰ. 일반기준\n    1. 둘 이상의 위반행위가 적발된 경우로서 위반행위가 다음 각 목의 어느 하\n  나에 해당하는 경우에는 가장 중한 정지처분 기간에 나머지 각각의 정지처분\n  기간의 2분의 1을 더하여 처분한다.\n     가. 영업정지에만 해당하는 경우\n  ┌───┬───┐\n  │ 가 │ 나 │\n  └───┴───┘";
        let out = reflow_wrapped(text);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[2], "  행정처분 기준(제89조 관련)");
        assert_eq!(lines[4], "  Ⅰ. 일반기준");
        assert_eq!(
            lines[5],
            "    1. 둘 이상의 위반행위가 적발된 경우로서 위반행위가 다음 각 목의 어느 하나에 해당하는 경우에는 가장 중한 정지처분 기간에 나머지 각각의 정지처분기간의 2분의 1을 더하여 처분한다."
        );
        assert_eq!(lines[6], "     가. 영업정지에만 해당하는 경우");
        assert_eq!(&lines[7..], &["  ┌───┬───┐", "  │ 가 │ 나 │", "  └───┴───┘"]);
    }

    #[test]
    fn joins_add_a_space_only_around_non_hangul_boundaries() {
        assert_eq!(reflow_wrapped("가나다 1호.\n  다음"), "가나다 1호. 다음");
        assert_eq!(reflow_wrapped("가나다(\n  라)"), "가나다(라)");
        assert_eq!(reflow_wrapped("가나다\n  , 라"), "가나다, 라");
        assert_eq!(reflow_wrapped("가나다\n\n  라"), "가나다\n\n  라");
        assert_eq!(reflow_wrapped("가나다\n   라"), "가나다\n   라");
        assert_eq!(reflow_wrapped("가나다\r\n  라마"), "가나다라마");
    }
}
