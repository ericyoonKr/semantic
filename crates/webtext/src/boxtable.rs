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

fn line_cells(line: &str) -> Vec<String> {
    let edge = |c: char| c == '┃' || c == '│';
    let t = line.trim();
    let t = t.strip_prefix(edge).unwrap_or(t);
    let t = t.strip_suffix(edge).unwrap_or(t);
    t.split(edge).map(|c| c.trim().to_string()).collect()
}

fn row_of(group: &[Vec<String>]) -> Vec<String> {
    let width = group.iter().map(Vec::len).max().unwrap_or(0);
    (0..width)
        .map(|j| group.iter().filter_map(|g| g.get(j)).filter(|c| !c.is_empty()).cloned().collect::<Vec<_>>().join("\n"))
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
        let cells = line_cells(line);
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
