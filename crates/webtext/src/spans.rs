use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Marked {
    pub text: String,
    pub marked: bool,
}

pub fn mark_spans(body: &str, spans: &[(usize, usize)]) -> Vec<Marked> {
    let chars: Vec<char> = body.chars().collect();
    let piece = |from: usize, to: usize| chars[from..to].iter().collect::<String>();
    let mut sorted = spans.to_vec();
    sorted.sort_by_key(|s| s.0);
    let mut out = Vec::new();
    let mut at = 0;
    for (start, end) in sorted {
        if start < at || end > chars.len() || start > end {
            continue;
        }
        if start > at {
            out.push(Marked { text: piece(at, start), marked: false });
        }
        out.push(Marked { text: piece(start, end), marked: true });
        at = end;
    }
    if at < chars.len() {
        out.push(Marked { text: piece(at, chars.len()), marked: false });
    }
    out
}
