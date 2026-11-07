use docmodel::{Anchor, Block, Cell, Document, FactSource, FileId, Table};

use super::of;

fn anchor(page: u32) -> Anchor {
    Anchor {
        source: FactSource::File(FileId("f".into())),
        page: Some(page),
        bbox: None,
        path: None,
        span: None,
    }
}

fn table(rows: &[&[&str]]) -> Block {
    let mut cells = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, text) in row.iter().enumerate() {
            cells.push(Cell {
                row: r as u32,
                col: c as u32,
                rowspan: 1,
                colspan: 1,
                text: (*text).to_string(),
                anchor: anchor(2),
            });
        }
    }
    Block::Table(Table {
        cells,
        anchor: anchor(2),
        degraded: false,
    })
}

fn paras(texts: &[&str]) -> Vec<Block> {
    texts
        .iter()
        .map(|t| Block::Para {
            text: (*t).to_string(),
            anchor: anchor(1),
        })
        .collect()
}

fn doc(blocks: Vec<Block>) -> Document {
    Document {
        blocks,
        source: FileId("f".into()),
        warnings: Vec::new(),
    }
}

const ARTICLES: [&str; 3] = [
    "제1조(목적) 이 계약은 용역의 범위와 대금을 정한다.",
    "제2조(기간) 계약기간은 1년으로 한다.",
    "제3조(대금) 대금은 매월 말일에 지급한다.",
];

fn contract(extra: &[&str]) -> Document {
    let mut texts: Vec<&str> = ARTICLES.to_vec();
    texts.extend_from_slice(extra);
    doc(paras(&texts))
}

fn right_samples() -> Vec<Document> {
    vec![
        doc(vec![table(&[&["항목", "금액"], &["가", "1,000,000원"], &["나", "2,000,000원"], &["합계", "3,000,000원"]])]),
        doc(vec![table(&[&["항목", "금액"], &["가", "금 일백만원"], &["나", "금 이백만원"], &["합계", "금 삼백만원"]])]),
        doc(vec![table(&[&["주주", "지분율"], &["갑", "70%"], &["을", "30%"]])]),
        doc(vec![table(&[&["주주", "지분율"], &["갑", "33.3%"], &["을", "33.3%"], &["병", "33.4%"]])]),
        doc(vec![table(&[&["항목", "금액"], &["가", "500"], &["나", "700"], &["다", "800"], &["계", "2,000"]])]),
        doc(vec![table(&[&["항목", "금액"], &["가", "500"], &["나", "700"]])]),
        contract(&["제4조(해지) 제2조의 기간 중에도 제3조를 어기면 해지할 수 있다."]),
        contract(&["제4조(책임) 민법 제750조에 따른 책임은 별도로 한다."]),
        contract(&["제4조(범위) 용역 범위는 별표 1에 따른다.", "[별표 1] 용역 범위"]),
        doc(paras(&["원고는 민법 제750조와 제751조에 따라 손해배상을 구한다.", "피고는 제3조를 다툰다."])),
    ]
}

fn wrong_samples() -> Vec<(Document, &'static str)> {
    vec![
        (doc(vec![table(&[&["항목", "금액"], &["가", "1,000,000원"], &["나", "2,500,000원"], &["합계", "3,000,000원"]])]), "sum_mismatch"),
        (doc(vec![table(&[&["항목", "금액"], &["가", "1,000,000원"], &["나", "2,000,000원"], &["합계", "3,000,001원"]])]), "sum_mismatch"),
        (doc(vec![table(&[&["항목", "금액"], &["가", "500"], &["나", "700"], &["다", "800"], &["계", "2,100"]])]), "sum_mismatch"),
        (doc(vec![table(&[&["항목", "금액"], &["가", "금 일백만원"], &["나", "금 이백만원"], &["합계", "금 사백만원"]])]), "sum_mismatch"),
        (doc(vec![table(&[&["주주", "지분율"], &["갑", "60%"], &["을", "30%"]])]), "ratio_mismatch"),
        (doc(vec![table(&[&["주주", "지분율"], &["갑", "60%"], &["을", "50%"]])]), "ratio_mismatch"),
        (contract(&["제4조(해지) 제9조의 사유가 있으면 해지할 수 있다."]), "orphan_reference"),
        (contract(&["제4조(정산) 정산은 제5조의2에 따른다."]), "orphan_reference"),
        (contract(&["제4조(범위) 용역 범위는 별표 1과 별표 2에 따른다.", "[별표 1] 용역 범위"]), "missing_annex"),
        (contract(&["제4조(신청) 신청은 별지 1 서식으로 한다."]), "missing_annex"),
    ]
}

#[test]
fn measured_false_alarms_and_misses_on_built_samples() {
    let right = right_samples();
    let false_alarms: Vec<usize> = right
        .iter()
        .enumerate()
        .filter(|(_, d)| !of(d).is_empty())
        .map(|(i, _)| i)
        .collect();
    let wrong = wrong_samples();
    let misses: Vec<usize> = wrong
        .iter()
        .enumerate()
        .filter(|(_, (d, kind))| !of(d).iter().any(|f| f.kind == *kind))
        .map(|(i, _)| i)
        .collect();
    eprintln!(
        "checks on upload: right {} samples, false alarms {} {:?}; wrong {} samples, misses {} {:?}",
        right.len(),
        false_alarms.len(),
        false_alarms,
        wrong.len(),
        misses.len(),
        misses
    );
    assert!(false_alarms.is_empty(), "{false_alarms:?}");
    assert!(misses.is_empty(), "{misses:?}");
}

#[test]
fn findings_carry_kind_message_and_page() {
    let found = of(&doc(vec![table(&[&["항목", "금액"], &["가", "1"], &["나", "2"], &["합계", "4"]])]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].kind, "sum_mismatch");
    assert_eq!(found[0].page, Some(2));
    assert!(!found[0].message.is_empty());
}
