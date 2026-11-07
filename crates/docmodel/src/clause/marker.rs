use regex::Regex;
use std::sync::LazyLock;

pub(super) enum Marker {
    Article { name: String, para: Option<u32> },
    Paragraph(u32),
    Item(u32),
}

static ARTICLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)^제([0-9]+)조(?:의([0-9]+))?([\s(].*|$)").expect("ARTICLE 정규식")
});
static PARA_AFTER_TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:\([^)]*\))?\s*([①-⑳])").expect("PARA_AFTER_TITLE 정규식"));
static PARA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([①-⑳])").expect("PARA 정규식"));
static ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9]+)\.(?:\s|$)").expect("ITEM 정규식"));
pub(super) fn parse_marker(text: &str) -> Option<Marker> {
    let t = text.trim_start();
    if let Some(c) = ARTICLE.captures(t) {
        let num = c.get(1)?.as_str();
        let name = match c.get(2) {
            Some(sub) => format!("제{num}조의{}", sub.as_str()),
            None => format!("제{num}조"),
        };
        let rest = c.get(3).map_or("", |m| m.as_str());
        let para = PARA_AFTER_TITLE
            .captures(rest)
            .and_then(|p| p.get(1))
            .and_then(|m| circled(m.as_str()));
        return Some(Marker::Article { name, para });
    }
    if let Some(m) = PARA.captures(t).and_then(|c| c.get(1)) {
        return circled(m.as_str()).map(Marker::Paragraph);
    }
    ITEM.captures(t)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
        .map(Marker::Item)
}
fn circled(s: &str) -> Option<u32> {
    let c = s.chars().next()?;
    Some(c as u32 - '①' as u32 + 1)
}
