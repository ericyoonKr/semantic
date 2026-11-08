use super::{AdjustResult, AdjustRule, Input, changes};
use crate::Error;
use crate::facts::{DateSpec, EventKind, Lookup};
use crate::resolve::{Missing, MissingReason, StepKind};

pub fn run(rule: &AdjustRule, input: &Input<'_>) -> Result<AdjustResult, Error> {
    let mut out = AdjustResult::default();
    let period = match input.facts.find(EventKind::TaxPeriod) {
        Lookup::Absent => return Ok(out),
        Lookup::Several => {
            out.missing.push(Missing {
                event: EventKind::TaxPeriod,
                reason: MissingReason::Several,
                step: StepKind::Adjust,
            });
            return Ok(out);
        }
        Lookup::One(f) => f.date,
    };
    let DateSpec::Span { from, to: Some(to) } = period else {
        out.missing.push(Missing {
            event: EventKind::TaxPeriod,
            reason: MissingReason::Imprecise,
            step: StepKind::Adjust,
        });
        return Ok(out);
    };
    if changes(input.store, input.unit, from, to)?.is_empty() {
        out.note(rule, input.graph, "과세기간 중 판본이 바뀌지 않음".to_string(), &[])?;
        return Ok(out);
    }
    let note = "과세기간 중에 판본이 바뀜 — 부진정소급으로 신뢰보호와 공익을 견주어 검토".to_string();
    out.review(rule, note.clone());
    out.note(rule, input.graph, note, &[])?;
    let at_start = input.store.at(input.unit, from)?;
    out.versions = Some(input.base.cloned().into_iter().chain(at_start).collect());
    Ok(out)
}
