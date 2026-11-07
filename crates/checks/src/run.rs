use docmodel::{Block, Clause, Document};

use crate::finding::Finding;
use crate::refs::{check_annexes, check_references};
use crate::sums::{check_ratios, check_table_sums};

pub fn run_all(doc: &Document, clauses: &[Clause]) -> Vec<Finding> {
    let mut out = Vec::new();
    for block in &doc.blocks {
        if let Block::Table(t) = block {
            out.extend(check_table_sums(t));
            out.extend(check_ratios(t));
        }
    }
    out.extend(check_references(clauses));
    out.extend(check_annexes(doc, clauses));
    out
}
