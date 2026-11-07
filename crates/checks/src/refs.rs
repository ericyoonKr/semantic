use std::collections::BTreeSet;
use std::sync::LazyLock;

use docmodel::{Block, Clause, Document};
use regex::Regex;

use crate::finding::{Finding, FindingKind, Severity};

const MIN_OWN_ARTICLES: usize = 3;

static OWN_ARTICLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(제[0-9]+조(?:의[0-9]+)?)").expect("OWN_ARTICLE 정규식"));
static ARTICLE_REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"제([0-9]+)조(?:의([0-9]+))?").expect("ARTICLE_REF 정규식"));
static FOREIGN_BEFORE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(법|법률|령|규칙|규정|조례|협약|헌법|「[^」]*」|같은\s*법|동법)\s*$").expect("FOREIGN_BEFORE 정규식")
});
static ANNEX_REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(별표|별지)\s*([0-9]+(?:의[0-9]+)?)").expect("ANNEX_REF 정규식"));
static ANNEX_HEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\[?\s*(별표|별지)\s*([0-9]+(?:의[0-9]+)?)").expect("ANNEX_HEAD 정규식"));

fn own_articles(clauses: &[Clause]) -> BTreeSet<String> {
    clauses
        .iter()
        .filter_map(|c| OWN_ARTICLE.captures(&c.id.0).and_then(|m| m.get(1)).map(|m| m.as_str().to_string()))
        .collect()
}

fn annex_heads(doc: &Document) -> BTreeSet<String> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading { text, .. } | Block::Para { text, .. } => ANNEX_HEAD.captures(text),
            Block::Table(_) => None,
        })
        .filter_map(|c| Some(format!("{} {}", c.get(1)?.as_str(), c.get(2)?.as_str())))
        .collect()
}

pub fn check_references(clauses: &[Clause]) -> Vec<Finding> {
    let own = own_articles(clauses);
    if own.len() < MIN_OWN_ARTICLES {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for clause in clauses {
        for m in ARTICLE_REF.captures_iter(&clause.text) {
            let Some(whole) = m.get(0) else { continue };
            if FOREIGN_BEFORE.is_match(&clause.text[..whole.start()]) {
                continue;
            }
            let name = whole.as_str().to_string();
            if own.contains(&name) || !seen.insert((clause.id.0.clone(), name.clone())) {
                continue;
            }
            out.push(Finding {
                kind: FindingKind::OrphanReference,
                message: format!("{name}을(를) 가리키지만 이 문서에 그 조항이 없습니다"),
                anchors: vec![clause.anchor.clone()],
                severity: Severity::Warning,
            });
        }
    }
    out
}

pub fn check_annexes(doc: &Document, clauses: &[Clause]) -> Vec<Finding> {
    let heads = annex_heads(doc);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for clause in clauses {
        for m in ANNEX_REF.captures_iter(&clause.text) {
            let (Some(kind), Some(no), Some(whole)) = (m.get(1), m.get(2), m.get(0)) else { continue };
            if FOREIGN_BEFORE.is_match(&clause.text[..whole.start()]) {
                continue;
            }
            let name = format!("{} {}", kind.as_str(), no.as_str());
            if heads.contains(&name) || !seen.insert(name.clone()) {
                continue;
            }
            out.push(Finding {
                kind: FindingKind::MissingAnnex,
                message: format!("{name}을(를) 가리키지만 이 문서에서 찾지 못했습니다"),
                anchors: vec![clause.anchor.clone()],
                severity: Severity::Warning,
            });
        }
    }
    out
}

#[cfg(test)]
#[path = "refs_tests.rs"]
mod tests;
