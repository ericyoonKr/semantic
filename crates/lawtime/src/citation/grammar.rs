use std::sync::OnceLock;

use chrono::NaiveDate;
use regex::{Captures, Regex};

use super::PromulRef;
use crate::Error;
use crate::versions::PromulNo;

const D: &str = r"(\d{4})\.\s*(\d{1,2})\.\s*(\d{1,2})\.?";
const P: &str = r"([가-힣]+)\s*제\s*(\d+)\s*호";
const VIA: &str = r"(?:\s+([^(),]+?))?";
const TO: &str = r"\s*(?:으로|로)\s*";
const KIND: &str = r"(일부\s*|전부\s*)?";
pub const ARTICLE: &str = r"(?:부칙\s*(?:\(\d{4}\.\s*\d{1,2}\.\s*\d{1,2}\.?\)\s*)?)?제\s*\d+\s*조(?:의\s*\d+)?(?:\s*제\s*\d+\s*항)?(?:\s*제\s*\d+\s*호)?(?:\s*\(?[가-하]\)?\s*목)?";

pub struct Grammar {
    pub upper_eff: Regex,
    pub both: Regex,
    pub upper: Regex,
    pub lower: Regex,
    pub promul: Regex,
    pub prose: Regex,
    pub unmarked: Regex,
    pub tail: Regex,
}

fn build() -> Result<Grammar, regex::Error> {
    let head = format!(r"^{D}\s*{P}{VIA}{TO}{KIND}");
    Ok(Grammar {
        upper_eff: Regex::new(&format!(r"{head}개정되어\s*{D}\s*시행되기\s*전의\s*것$"))?,
        both: Regex::new(&format!(
            r"{head}개정되고\s*,?\s*{D}\s*{P}{VIA}{TO}{KIND}개정되기\s*전의\s*것$"
        ))?,
        upper: Regex::new(&format!(r"{head}(개정|폐지)되기\s*전의\s*것$"))?,
        lower: Regex::new(&format!(r"{head}개정(?:되어\s*{D}\s*시행)?된\s*것$"))?,
        promul: Regex::new(&format!(r"^{D}\s*{P}$"))?,
        prose: Regex::new(&format!(
            r"{D}\s*{P}{TO}(?:일부\s*|전부\s*)?개정되어\s*{D}\s*부터\s*시행된\s+([^,()]+?)\s+({ARTICLE})"
        ))?,
        unmarked: Regex::new(&format!(r"^\s*(구\s+)?([^()]+?)\s+({ARTICLE})"))?,
        tail: Regex::new(r"\s*,\s*이하\s*(같다|[^,]*?라\s*(?:고\s*)?한다)\s*$")?,
    })
}

pub fn grammar() -> Result<&'static Grammar, Error> {
    static CELL: OnceLock<Result<Grammar, String>> = OnceLock::new();
    CELL.get_or_init(|| build().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| Error::Regex(e.clone()))
}

fn num<T: std::str::FromStr>(c: &Captures<'_>, i: usize) -> Option<T> {
    c.get(i)?.as_str().parse().ok()
}

pub fn date(c: &Captures<'_>, first: usize) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(num(c, first)?, num(c, first + 1)?, num(c, first + 2)?)
}

pub fn promul(c: &Captures<'_>, date_at: usize, via_at: Option<usize>) -> Option<PromulRef> {
    let via = via_at.and_then(|i| c.get(i)).map(|m| m.as_str().trim().to_string());
    if via.as_deref().is_some_and(|v| v.contains("개정") || v.contains("호로")) {
        return None;
    }
    Some(PromulRef {
        date: date(c, date_at)?,
        no: PromulNo::new(c.get(date_at + 3)?.as_str(), num(c, date_at + 4)?),
        via,
    })
}

pub fn full(c: &Captures<'_>, i: usize) -> bool {
    c.get(i).is_some_and(|m| m.as_str().starts_with("전부"))
}
