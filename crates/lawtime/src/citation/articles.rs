use std::sync::OnceLock;

use regex::Regex;

use crate::Error;

const JO: &str = r"((?:부칙\s*(?:\(\d{4}\.\s*\d{1,2}\.\s*\d{1,2}\.?\)\s*)?)?제\s*\d+\s*조(?:의\s*\d+)?|부칙\s*(?:\(\d{4}\.\s*\d{1,2}\.\s*\d{1,2}\.?\))?)";
const HANG: &str = r"(제\s*\d+\s*항)";
const HO: &str = r"(제\s*\d+\s*호(?:의\s*\d+)?)";
const MOK: &str = r"(\(?[가-하]\)?\s*목)";

struct Patterns {
    item: Regex,
    sep: Regex,
    range: Regex,
    anaphora: Regex,
}

fn patterns() -> Result<&'static Patterns, Error> {
    static CELL: OnceLock<Result<Patterns, String>> = OnceLock::new();
    CELL.get_or_init(|| {
        let build = || -> Result<Patterns, regex::Error> {
            Ok(Patterns {
                item: Regex::new(&format!(r"^\s*{JO}?(?:\s*{HANG})?(?:\s*{HO})?(?:\s*{MOK})?"))?,
                sep: Regex::new(r"^\s*(?:,|및|ㆍ)\s*")?,
                range: Regex::new(r"^\s*(?:부터|내지|～|~)")?,
                anaphora: Regex::new(r"^(?:같은\s*(?:법|조|항|호|목|규칙|시행령|령)|동\s*(?:법|조|항|호))")?,
            })
        };
        build().map_err(|e| e.to_string())
    })
    .as_ref()
    .map_err(|e| Error::Regex(e.clone()))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArticleList {
    pub items: Vec<String>,
    pub unresolved: bool,
    pub end: usize,
}

#[derive(Default)]
struct Context {
    jo: Option<String>,
    hang: Option<String>,
    ho: Option<String>,
}

fn take(c: &regex::Captures<'_>, i: usize) -> Option<String> {
    c.get(i).map(|m| m.as_str().trim().to_string())
}

fn read_item(text: &str, ctx: &mut Context) -> Result<Option<(String, usize)>, Error> {
    let Some(c) = patterns()?.item.captures(text) else { return Ok(None) };
    let parts = [take(&c, 1), take(&c, 2), take(&c, 3), take(&c, 4)];
    let Some(first) = parts.iter().position(Option::is_some) else { return Ok(None) };
    let inherited = [ctx.jo.clone(), ctx.hang.clone(), ctx.ho.clone()];
    if first > 0 && ctx.jo.is_none() {
        return Ok(None);
    }
    let mut full: Vec<String> = inherited[..first].iter().flatten().cloned().collect();
    full.extend(parts[first..].iter().flatten().cloned());
    let level = |i: usize| if i < first { inherited[i].clone() } else { parts[i].clone() };
    *ctx = Context { jo: level(0), hang: level(1), ho: level(2) };
    let end = c.get(0).map_or(0, |m| m.end());
    Ok(Some((full.join(" "), end)))
}

fn skip_note(text: &str, pos: usize) -> usize {
    let rest = &text[pos..];
    let trimmed = rest.trim_start();
    if !trimmed.starts_with('(') {
        return pos;
    }
    let start = pos + rest.len() - trimmed.len();
    let mut depth = 0usize;
    for (i, ch) in text[start..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return start + i + 1;
                }
            }
            _ => {}
        }
    }
    pos
}

pub fn read_list(text: &str, from: usize) -> Result<ArticleList, Error> {
    let p = patterns()?;
    let mut ctx = Context::default();
    let mut out = ArticleList { end: from, ..Default::default() };
    let Some((first, len)) = read_item(&text[from..], &mut ctx)? else { return Ok(out) };
    out.items.push(first);
    out.end = from + len;
    let mut pos = out.end;
    loop {
        pos = skip_note(text, pos);
        if p.range.is_match(&text[pos..]) {
            out.unresolved = true;
            break;
        }
        let Some(sep) = p.sep.find(&text[pos..]) else { break };
        let after = pos + sep.end();
        if p.anaphora.is_match(&text[after..]) {
            out.unresolved = true;
            break;
        }
        let Some((item, len)) = read_item(&text[after..], &mut ctx)? else { break };
        out.items.push(item);
        out.end = after + len;
        pos = out.end;
    }
    Ok(out)
}
