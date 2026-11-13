use std::collections::{HashMap, HashSet};

use serde::Serialize;

pub const SPLIT_VERSION: u32 = 1;
pub const PASSAGE_CHARS: usize = 600;
pub const RRF_K: f32 = 60.0;
const LITERAL_BONUS: f32 = 2.0;
const WORD_MIN: usize = 2;
const PARTICLES: [&str; 14] = ["에서", "으로", "이란", "은", "는", "이", "가", "을", "를", "에", "의", "로", "도", "와"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Passage {
    pub page: Option<u32>,
    pub text: String,
}

fn page_mark(line: &str) -> Option<u32> {
    line.trim().strip_prefix("--- p")?.strip_suffix(" ---")?.parse().ok()
}

fn flush(out: &mut Vec<Passage>, page: Option<u32>, buf: &mut String) {
    let text = buf.trim();
    if !text.is_empty() {
        out.push(Passage { page, text: text.to_string() });
    }
    buf.clear();
}

pub fn passages(text: &str) -> Vec<Passage> {
    let mut out = Vec::new();
    let mut page = None;
    let mut buf = String::new();
    for line in text.lines() {
        if let Some(p) = page_mark(line) {
            flush(&mut out, page, &mut buf);
            page = Some(p);
            continue;
        }
        let mut rest: Vec<char> = line.chars().collect();
        while rest.len() > PASSAGE_CHARS {
            let head: String = rest.drain(..PASSAGE_CHARS).collect();
            buf.push_str(&head);
            flush(&mut out, page, &mut buf);
        }
        buf.extend(rest);
        buf.push('\n');
        if buf.chars().count() >= PASSAGE_CHARS {
            flush(&mut out, page, &mut buf);
        }
    }
    flush(&mut out, page, &mut buf);
    out
}

fn bigrams(text: &str) -> HashSet<String> {
    let chars: Vec<char> = text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    chars.windows(2).map(|w| w.iter().collect()).collect()
}

pub fn query_words(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .map(|w| PARTICLES.iter().find_map(|p| w.strip_suffix(p).filter(|r| r.chars().count() >= WORD_MIN)).unwrap_or(w))
        .filter(|w| w.chars().count() >= WORD_MIN)
        .map(str::to_string)
        .collect()
}

pub fn keyword_scores(query: &str, passages: &[Passage]) -> Vec<(usize, f32)> {
    let grams: Vec<HashSet<String>> = passages.iter().map(|p| bigrams(&p.text)).collect();
    let wanted = bigrams(query);
    let n = passages.len().max(1) as f32;
    let mut df: HashMap<&str, f32> = HashMap::new();
    for g in &wanted {
        df.insert(g.as_str(), grams.iter().filter(|set| set.contains(g)).count() as f32);
    }
    let words = query_words(query);
    let mut out: Vec<(usize, f32)> = passages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let overlap: f32 = wanted.iter().filter(|g| grams[i].contains(*g)).map(|g| (n / (1.0 + df[g.as_str()])).ln().max(0.0) + 1.0).sum();
            let literal = words.iter().filter(|w| p.text.contains(w.as_str())).count() as f32 * LITERAL_BONUS;
            (i, overlap + literal)
        })
        .filter(|(_, s)| *s > 0.0)
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}

pub fn vector_scores(query: &[f32], vectors: &[Vec<f32>]) -> Vec<(usize, f32)> {
    let mut out: Vec<(usize, f32)> = vectors.iter().enumerate().map(|(i, v)| (i, cosine(query, v))).collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}

pub fn fuse(lists: &[Vec<(usize, f32)>], depth: usize) -> Vec<(usize, f32)> {
    let mut score: HashMap<usize, f32> = HashMap::new();
    for list in lists {
        for (rank, (i, _)) in list.iter().take(depth).enumerate() {
            *score.entry(*i).or_default() += 1.0 / (RRF_K + rank as f32 + 1.0);
        }
    }
    let mut out: Vec<(usize, f32)> = score.into_iter().collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

pub fn encode_vectors(vectors: &[Vec<f32>]) -> Vec<u8> {
    let dim = vectors.first().map_or(0, Vec::len);
    let mut out = Vec::with_capacity(12 + vectors.len() * dim * 4);
    for n in [SPLIT_VERSION, vectors.len() as u32, dim as u32] {
        out.extend_from_slice(&n.to_le_bytes());
    }
    for v in vectors {
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
    out
}

pub fn decode_vectors(bytes: &[u8], count: usize) -> Option<Vec<Vec<f32>>> {
    let word = |i: usize| bytes.get(i * 4..i * 4 + 4).and_then(|b| b.try_into().ok()).map(u32::from_le_bytes);
    let (version, stored, dim) = (word(0)?, word(1)? as usize, word(2)? as usize);
    if version != SPLIT_VERSION || stored != count || dim == 0 || bytes.len() != 12 + stored * dim * 4 {
        return None;
    }
    let (words, _) = bytes[12..].as_chunks::<4>();
    let floats: Vec<f32> = words.iter().map(|b| f32::from_le_bytes(*b)).collect();
    Some(floats.chunks(dim).map(<[f32]>::to_vec).collect())
}

#[cfg(test)]
mod tests;
