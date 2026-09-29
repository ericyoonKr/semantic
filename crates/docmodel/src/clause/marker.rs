// 조·항·호 표시 판별 — clause.rs가 부른다
// 블록 글자의 맨 앞에서 조("제5조", "제5조의2")·항(①~⑳)·호("1.")를 찾는다.
// 표시 종류가 늘면(목 "가.", "(1)", 부칙·별표) 이 파일만 고친다.
// 오탐·누락은 실제 계약서·처분서 표본으로 잰다 (IMPLEMENTATION_ORDER.md 4단계).
// 알려진 오탐: 「제5조 및 제6조에 따라…」처럼 조 번호 뒤에 공백이 오는 인용문.

use regex::Regex;
use std::sync::LazyLock;

pub(super) enum Marker {
    Article { name: String, para: Option<u32> }, // "제5조", "제5조의2" (+ 같은 문단의 ①)
    Paragraph(u32),                              // ①~⑳
    Item(u32),                                   // "1." "12."
}

static ARTICLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)^제([0-9]+)조(?:의([0-9]+))?([\s(].*|$)").expect("ARTICLE 정규식")
});
// // 조 제목 "(목적)" 다음에 바로 오는 항 표시
static PARA_AFTER_TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:\([^)]*\))?\s*([①-⑳])").expect("PARA_AFTER_TITLE 정규식"));
// // 항: ①~⑳
static PARA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([①-⑳])").expect("PARA 정규식"));
// // 호: "1." "12." — 점 뒤가 공백·끝일 때만 (「1.5%」 제외)
static ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9]+)\.(?:\s|$)").expect("ITEM 정규식"));
//
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
// /// "①"(U+2460) → 1 … "⑳"(U+2473) → 20
fn circled(s: &str) -> Option<u32> {
    let c = s.chars().next()?;
    Some(c as u32 - '①' as u32 + 1)
}
