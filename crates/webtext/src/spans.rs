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

fn folded(text: &str) -> Vec<(char, usize)> {
    text.chars().enumerate().filter(|(_, c)| !c.is_whitespace()).flat_map(|(i, c)| c.to_lowercase().map(move |l| (l, i))).collect()
}

pub fn find_all(body: &str, query: &str) -> Vec<(usize, usize)> {
    let needle: Vec<char> = folded(query).into_iter().map(|(c, _)| c).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let hay = folded(body);
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()].iter().zip(&needle).all(|((h, _), n)| h == n) {
            out.push((hay[i].1, hay[i + needle.len() - 1].1 + 1));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Layered {
    pub text: String,
    pub cite: bool,
    pub find: bool,
    pub current: bool,
}

pub fn mark_layers(body: &str, cites: &[(usize, usize)], finds: &[(usize, usize)], current: Option<usize>) -> Vec<Layered> {
    let chars: Vec<char> = body.chars().collect();
    let len = chars.len();
    let valid = |&(start, end): &(usize, usize)| start < end && end <= len;
    let layers: [Vec<(usize, usize)>; 3] = [
        cites.iter().copied().filter(valid).collect(),
        finds.iter().copied().filter(valid).collect(),
        current.and_then(|i| finds.get(i)).copied().filter(valid).into_iter().collect(),
    ];
    let mut bounds = vec![0, len];
    let mut events = Vec::new();
    for (layer, ranges) in layers.iter().enumerate() {
        for &(start, end) in ranges {
            bounds.push(start);
            bounds.push(end);
            events.push((start, layer, 1i32));
            events.push((end, layer, -1i32));
        }
    }
    bounds.sort_unstable();
    bounds.dedup();
    events.sort_by_key(|e| e.0);
    let mut depth = [0i32; 3];
    let mut next = 0;
    let mut out = Vec::new();
    for pair in bounds.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        while next < events.len() && events[next].0 <= from {
            let (_, layer, delta) = events[next];
            depth[layer] += delta;
            next += 1;
        }
        out.push(Layered { text: chars[from..to].iter().collect(), cite: depth[0] > 0, find: depth[1] > 0, current: depth[2] > 0 });
    }
    out
}
