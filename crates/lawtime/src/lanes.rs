use serde::Serialize;

use crate::citation::{CiteStatus, CitationResolution};
use crate::resolve::Resolution;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LaneOutcome {
    Same,
    AmongSeveral,
    Different,
    NoAnswer,
    NoPrediction,
}

pub fn compare(answer: &CitationResolution, predicted: &Resolution) -> LaneOutcome {
    let Some(a) = answer.version.as_ref().filter(|_| answer.status == CiteStatus::Resolved) else {
        return LaneOutcome::NoAnswer;
    };
    if predicted.versions.is_empty() {
        return LaneOutcome::NoPrediction;
    }
    let hit = predicted.versions.iter().any(|v| v.same_row(a) && v.promul_no == a.promul_no);
    match (hit, predicted.versions.len()) {
        (true, 1) => LaneOutcome::Same,
        (true, _) => LaneOutcome::AmongSeveral,
        (false, _) => LaneOutcome::Different,
    }
}
