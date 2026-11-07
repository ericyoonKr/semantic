use checks::FindingKind;
use docmodel::{Document, assign_clauses};

use crate::upload::record::Finding;

fn kind_name(kind: FindingKind) -> &'static str {
    match kind {
        FindingKind::SumMismatch => "sum_mismatch",
        FindingKind::RatioMismatch => "ratio_mismatch",
        FindingKind::OrphanReference => "orphan_reference",
        FindingKind::MissingAnnex => "missing_annex",
    }
}

pub fn of(doc: &Document) -> Vec<Finding> {
    let clauses = assign_clauses(doc);
    checks::run_all(doc, &clauses)
        .into_iter()
        .map(|f| Finding {
            kind: kind_name(f.kind).to_string(),
            page: f.anchors.iter().find_map(|a| a.page),
            message: f.message,
        })
        .collect()
}

#[cfg(test)]
#[path = "findings_tests.rs"]
mod tests;
