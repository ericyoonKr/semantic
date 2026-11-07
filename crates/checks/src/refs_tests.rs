use docmodel::{Anchor, Block, Document, FactSource, FileId, assign_clauses};

use super::{check_annexes, check_references};
use crate::finding::FindingKind;

fn anchor(i: usize) -> Anchor {
    Anchor { source: FactSource::File(FileId("f".into())), page: Some(1), bbox: None, path: Some(format!("p[{i}]")), span: None }
}

fn doc(paras: &[&str]) -> Document {
    Document {
        blocks: paras.iter().enumerate().map(|(i, t)| Block::Para { text: (*t).to_string(), anchor: anchor(i) }).collect(),
        source: FileId("f".into()),
        warnings: Vec::new(),
    }
}

#[test]
fn orphan_reference_is_flagged_and_statute_citation_is_not() {
    let d = doc(&[
        "제1조(목적) 이 계약은 용역에 관한 사항을 정한다.",
        "제2조(대금) 대금은 제3조에 따라 지급한다.",
        "제3조(지급) 대금은 매월 말일에 지급한다. 민법 제750조에 따른 책임은 별도로 한다.",
        "제4조(해지) 제9조의 사유가 있으면 해지할 수 있다.",
    ]);
    let clauses = assign_clauses(&d);
    let f = check_references(&clauses);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].kind, FindingKind::OrphanReference);
    assert!(f[0].message.contains("제9조"));
}

#[test]
fn documents_without_own_articles_are_skipped() {
    let d = doc(&["원고는 민법 제750조와 제751조에 따라 손해배상을 구한다.", "피고는 제3조를 다툰다."]);
    assert!(check_references(&assign_clauses(&d)).is_empty());
}

#[test]
fn missing_annex_is_flagged() {
    let d = doc(&[
        "제1조(범위) 범위는 별표 1과 별표 2에 따른다.",
        "제2조(기간) 기간은 1년으로 한다.",
        "제3조(기타) 시행규칙 별표 3에 따른다.",
        "[별표 1] 용역 범위",
    ]);
    let f = check_annexes(&d, &assign_clauses(&d));
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].message.contains("별표 2"));
}
