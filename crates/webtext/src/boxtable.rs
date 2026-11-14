use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoxTable {
    pub head: Vec<Vec<String>>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Piece {
    Text { text: String, start: usize, end: usize },
    Table { table: BoxTable, start: usize, end: usize },
}

fn starts_with_any(line: &str, marks: &[char]) -> bool {
    line.trim_start().chars().next().is_some_and(|c| marks.contains(&c))
}

fn is_top(line: &str) -> bool {
    starts_with_any(line, &['┏'])
}

fn is_bottom(line: &str) -> bool {
    starts_with_any(line, &['┗'])
}

fn is_body_start(line: &str) -> bool {
    starts_with_any(line, &['┠', '┣'])
}

fn is_separator(line: &str) -> bool {
    line.chars().any(|c| c == '─' || c == '━') && !line.chars().any(char::is_alphanumeric)
}

const RULES: [char; 15] = ['─', '━', '┬', '┴', '┼', '├', '┤', '┨', '┠', '┣', '┫', '┏', '┓', '┗', '┛'];
const ITEM_MARKS: [char; 4] = ['○', '▪', '•', '※'];
const CONNECTORS: [char; 8] = ['·', 'ㆍ', '・', '/', '~', '-', '(', ')'];
const WORD_STARTS: [&str; 5] = ["및", "또는", "그리고", "다만", "단,"];

fn raw_cells(line: &str) -> Vec<String> {
    let edge = |c: char| c == '┃' || c == '│';
    let t = line.trim();
    let t = t.strip_prefix(edge).unwrap_or(t);
    let t = t.strip_suffix(edge).unwrap_or(t);
    t.split(edge).map(str::to_string).collect()
}

fn ruled(cell: &str) -> bool {
    cell.chars().any(|c| RULES.contains(&c))
}

fn before_rule(cell: &str) -> String {
    clean(cell.find(|c| RULES.contains(&c)).map_or(cell, |at| &cell[..at]))
}

fn clean(cell: &str) -> String {
    cell.chars().map(|ch| if RULES.contains(&ch) { ' ' } else { ch }).collect::<String>().trim().to_string()
}

fn cjk(c: char) -> bool {
    ('가'..='힣').contains(&c) || ('一'..='鿿').contains(&c) || ('ㄱ'..='ㆎ').contains(&c)
}

fn starts_item(text: &str) -> bool {
    let mut chars = text.chars();
    let (Some(first), second) = (chars.next(), chars.next()) else { return false };
    if ITEM_MARKS.contains(&first) {
        return true;
    }
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    if !digits.is_empty() {
        return matches!(text[digits.len()..].chars().next(), Some(')') | Some('.'));
    }
    cjk(first) && matches!(second, Some('.') | Some(')'))
}

fn join_cell(parts: &[&str]) -> String {
    let mut out = String::new();
    for part in parts {
        if out.is_empty() {
            out.push_str(part);
            continue;
        }
        let prev = out.chars().next_back();
        let next = part.chars().next();
        let glued = prev.is_some_and(|p| CONNECTORS.contains(&p)) || next.is_some_and(|n| CONNECTORS.contains(&n));
        let wrapped = prev.is_some_and(|p| cjk(p) || p.is_ascii_digit()) && next.is_some_and(cjk) && !WORD_STARTS.iter().any(|w| part.starts_with(w));
        if starts_item(part) {
            out.push('\n');
        } else if !(glued || wrapped) {
            out.push(' ');
        }
        out.push_str(part);
    }
    out
}

fn row_of(group: &[Vec<String>]) -> Vec<String> {
    let width = group.iter().map(Vec::len).max().unwrap_or(0);
    (0..width)
        .map(|j| {
            let parts: Vec<&str> = group.iter().filter_map(|g| g.get(j)).filter(|c| !c.is_empty()).map(String::as_str).collect();
            join_cell(&parts)
        })
        .collect()
}

pub fn parse_box_table(lines: &[&str]) -> BoxTable {
    let mut head = Vec::new();
    let mut rows = Vec::new();
    let mut in_body = !lines.iter().any(|l| is_body_start(l));
    let mut group: Vec<Vec<String>> = Vec::new();
    let flush = |group: &mut Vec<Vec<String>>, in_body: bool, head: &mut Vec<Vec<String>>, rows: &mut Vec<Vec<String>>| {
        if !group.is_empty() {
            let row = row_of(group);
            if row.iter().any(|c| !c.is_empty()) {
                if in_body { rows.push(row) } else { head.push(row) }
            }
        }
        group.clear();
    };
    for line in lines {
        if is_top(line) || is_bottom(line) {
            flush(&mut group, in_body, &mut head, &mut rows);
            continue;
        }
        if is_separator(line) {
            flush(&mut group, in_body, &mut head, &mut rows);
            in_body |= is_body_start(line);
            continue;
        }
        let raw = raw_cells(line);
        let cells: Vec<String> = raw.iter().map(|c| clean(c)).collect();
        if raw.iter().any(|c| ruled(c)) {
            let carried: Vec<String> = raw.iter().zip(&cells).map(|(r, c)| if ruled(r) { before_rule(r) } else { c.clone() }).collect();
            if carried.iter().any(|c| !c.is_empty()) {
                group.push(carried);
            }
            flush(&mut group, in_body, &mut head, &mut rows);
            continue;
        }
        if cells.iter().all(String::is_empty) {
            flush(&mut group, in_body, &mut head, &mut rows);
            continue;
        }
        group.push(cells);
    }
    flush(&mut group, in_body, &mut head, &mut rows);
    BoxTable { head, rows }
}

pub fn split_body(body: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut text_start = 0;
    let mut table: Option<(usize, Vec<&str>)> = None;
    let mut at = 0;
    let lines: Vec<&str> = body.split('\n').collect();
    let last = lines.len().saturating_sub(1);
    for (i, line) in lines.iter().enumerate() {
        let len = line.chars().count() + usize::from(i < last);
        if table.is_none() && is_top(line) {
            if !text.is_empty() {
                out.push(Piece::Text { text: std::mem::take(&mut text), start: text_start, end: at });
            }
            table = Some((at, Vec::new()));
        }
        if let Some((start, rows)) = table.as_mut() {
            rows.push(line);
            at += len;
            if is_bottom(line) {
                out.push(Piece::Table { table: parse_box_table(rows), start: *start, end: at });
                table = None;
                text_start = at;
            }
            continue;
        }
        if text.is_empty() {
            text_start = at;
        }
        text.push_str(line);
        if i < last {
            text.push('\n');
        }
        at += len;
    }
    if let Some((start, rows)) = table {
        out.push(Piece::Table { table: parse_box_table(&rows), start, end: at });
    }
    if !text.is_empty() {
        out.push(Piece::Text { text, start: text_start, end: at });
    }
    out
}
