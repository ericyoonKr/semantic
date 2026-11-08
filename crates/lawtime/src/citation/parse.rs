use super::grammar::{date, full, grammar, promul};
use super::{Bound, Citation};
use crate::Error;

const DELIMS: [char; 10] = [',', '/', '(', ')', '[', ']', ';', '\n', '】', '、'];

pub fn parse_bracket(inner: &str) -> Result<Option<(Bound, bool)>, Error> {
    let g = grammar()?;
    let (body, same_below) = match g.tail.captures(inner) {
        Some(c) => {
            let start = c.get(0).map_or(inner.len(), |m| m.start());
            (&inner[..start], c.get(1).is_some_and(|m| m.as_str() == "같다"))
        }
        None => (inner, false),
    };
    let body = body.trim();
    let bound = if let Some(c) = g.upper_eff.captures(body) {
        promul(&c, 1, Some(6)).zip(date(&c, 8)).map(|(p, e)| Bound::UpperWithEffective {
            promul: p,
            effective: e,
            full: full(&c, 7),
        })
    } else if let Some(c) = g.both.captures(body) {
        promul(&c, 1, Some(6)).zip(promul(&c, 8, Some(13))).map(|(lower, upper)| Bound::Both {
            lower,
            upper,
            lower_full: full(&c, 7),
            upper_full: full(&c, 14),
        })
    } else if let Some(c) = g.upper.captures(body) {
        let repeal = c.get(8).is_some_and(|m| m.as_str() == "폐지");
        promul(&c, 1, Some(6)).map(|p| match (repeal, full(&c, 7)) {
            (false, true) => Bound::FullRevisionUpper { promul: p },
            _ => Bound::Upper { promul: p, repeal },
        })
    } else if let Some(c) = g.lower.captures(body) {
        promul(&c, 1, Some(6)).map(|p| Bound::Lower { promul: p, full: full(&c, 7), effective: date(&c, 8) })
    } else if let Some(c) = g.promul.captures(body) {
        promul(&c, 1, None).map(|p| Bound::AsPromulgated { promul: p })
    } else {
        None
    };
    Ok(bound.map(|b| (b, same_below)))
}

fn brackets(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for (i, ch) in text.char_indices() {
        match ch {
            '(' => stack.push(i),
            ')' => {
                if let Some(open) = stack.pop() {
                    out.push((open, i));
                }
            }
            _ => {}
        }
    }
    out.sort_unstable();
    out
}

fn split_old(raw: &str) -> (String, bool) {
    let mut name = raw.trim();
    let mut old = false;
    if let Some(i) = name.rfind("구 ") {
        let boundary = name[..i].chars().next_back().is_none_or(char::is_whitespace);
        if boundary {
            name = &name[i + "구 ".len()..];
            old = true;
        }
    }
    for p in ["및 ", "또는 "] {
        if let Some(rest) = name.strip_prefix(p) {
            name = rest;
        }
    }
    (name.trim().to_string(), old)
}

fn law_before(text: &str, open: usize) -> Option<(String, bool)> {
    let before = text[..open].trim_end();
    let (name, old) = if let Some(inner) = before.strip_suffix('」') {
        let s = inner.rfind('「')?;
        let (n, _) = split_old(&inner[s + '「'.len_utf8()..]);
        (n, inner[..s].trim_end().ends_with('구'))
    } else {
        let start = before
            .char_indices()
            .rev()
            .find(|(_, c)| DELIMS.contains(c))
            .map_or(0, |(i, c)| i + c.len_utf8());
        split_old(&before[start..])
    };
    (!name.is_empty()).then_some((name, old))
}

fn article_after(text: &str, close: usize) -> Result<(Option<String>, usize), Error> {
    let rest = &text[close + 1..];
    Ok(match grammar()?.article_at.captures(rest).and_then(|c| c.get(1)) {
        Some(m) => (Some(m.as_str().to_string()), close + 1 + m.end()),
        None => (None, close + 1),
    })
}

fn inherited(text: &str, from: &Citation, taken: &[(usize, usize)]) -> Result<Vec<Citation>, Error> {
    let mut out = Vec::new();
    for (pos, _) in text[from.end..].match_indices(from.law_name.as_str()) {
        let start = from.end + pos;
        let end_name = start + from.law_name.len();
        if taken.iter().any(|(s, e)| start >= *s && start < *e) {
            continue;
        }
        let prev = text[..start].chars().next_back();
        if !prev.is_none_or(|c| c.is_whitespace() || DELIMS.contains(&c)) {
            continue;
        }
        let rest = &text[end_name..];
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let Some(m) = grammar()?.article_at.captures(rest).and_then(|c| c.get(1)) else { continue };
        let old = text[..start].trim_end().ends_with(" 구") || text[..start].trim_end() == "구";
        out.push(Citation {
            law_name: from.law_name.clone(),
            old,
            bound: from.bound.clone(),
            article: Some(m.as_str().to_string()),
            same_below: false,
            inherited: true,
            start,
            end: end_name + m.end(),
        });
    }
    Ok(out)
}

pub fn parse_citations(text: &str) -> Result<Vec<Citation>, Error> {
    let mut cites = Vec::new();
    for (open, close) in brackets(text) {
        let Some((bound, same_below)) = parse_bracket(&text[open + 1..close])? else { continue };
        let Some((law_name, old)) = law_before(text, open) else { continue };
        let (article, end) = article_after(text, close)?;
        let start = text[..open].rfind(law_name.as_str()).unwrap_or(open);
        cites.push(Citation { law_name, old, bound, article, same_below, inherited: false, start, end });
    }
    for c in grammar()?.prose.captures_iter(text) {
        let (Some(whole), Some(p), Some(e), Some(name), Some(art)) =
            (c.get(0), promul(&c, 1, None), date(&c, 6), c.get(9), c.get(10))
        else {
            continue;
        };
        let (law_name, old) = split_old(name.as_str());
        cites.push(Citation {
            law_name,
            old,
            bound: Bound::Prose { promul: p, effective: e },
            article: Some(art.as_str().to_string()),
            same_below: false,
            inherited: false,
            start: whole.start(),
            end: whole.end(),
        });
    }
    let taken: Vec<(usize, usize)> = cites.iter().map(|c| (c.start, c.end)).collect();
    let mut extra = Vec::new();
    for c in cites.iter().filter(|c| c.same_below) {
        extra.extend(inherited(text, c, &taken)?);
    }
    cites.extend(extra);
    cites.sort_by_key(|c| c.start);
    Ok(cites)
}

pub fn parse_one(text: &str) -> Result<Option<Citation>, Error> {
    if let Some(c) = parse_citations(text)?.into_iter().next() {
        return Ok(Some(c));
    }
    if text.contains('(') {
        return Ok(None);
    }
    let Some(c) = grammar()?.unmarked.captures(text) else { return Ok(None) };
    let (Some(name), Some(art)) = (c.get(2), c.get(3)) else { return Ok(None) };
    Ok(Some(Citation {
        law_name: name.as_str().trim().to_string(),
        old: c.get(1).is_some(),
        bound: Bound::Unmarked,
        article: Some(art.as_str().to_string()),
        same_below: false,
        inherited: false,
        start: 0,
        end: text.len(),
    }))
}
