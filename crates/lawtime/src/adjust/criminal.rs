use super::{AdjustResult, AdjustRule, Doctrine, Input, changes, differs};
use crate::Error;
use crate::facts::{EventKind, Lookup};
use crate::resolve::{Missing, MissingReason, StepKind};
use crate::versions::EndReason;

fn after_final(rule: &AdjustRule, input: &Input<'_>, out: &mut AdjustResult) -> Result<(), Error> {
    if matches!(input.facts.state, Some(crate::facts::ProcState::Finalized))
        && input.facts.find(EventKind::Finalized) == Lookup::Absent
    {
        out.missing.push(Missing {
            event: EventKind::Finalized,
            reason: MissingReason::Absent,
            step: StepKind::Adjust,
        });
        return Ok(());
    }
    let Some(f) = out.point(input.facts, EventKind::Finalized, false) else { return Ok(()) };
    let at_f = input.store.at(input.unit, f)?;
    let at_now = input.store.at(input.unit, input.as_of)?;
    if !changes(input.store, input.unit, f, input.as_of)?.is_empty() || differs(at_f.as_ref(), at_now.as_ref()) {
        out.review(
            rule,
            "재판 확정 후 판본이 바뀜 — 범죄가 되지 않게 되었으면 형의 집행 면제 검토".to_string(),
        );
        out.note(rule, input.graph, "확정 후 변경 있음".to_string(), &["X:확정후집행면제"])?;
    }
    Ok(())
}

pub fn run(rule: &AdjustRule, input: &Input<'_>) -> Result<AdjustResult, Error> {
    let mut out = AdjustResult::default();
    let Some(base) = input.base else {
        out.note(rule, input.graph, "행위시 판본이 없어 보정하지 않음".to_string(), &[])?;
        return Ok(out);
    };
    let Some(t) = input.base_date.upper() else {
        out.missing.push(Missing {
            event: EventKind::Act,
            reason: MissingReason::Imprecise,
            step: StepKind::Adjust,
        });
        return Ok(out);
    };
    let Some(j) = out.point(input.facts, EventKind::Judgment, true) else { return Ok(out) };
    let at_j = input.store.at(input.unit, j)?;
    let changed = !changes(input.store, input.unit, t, j)?.is_empty() || differs(Some(base), at_j.as_ref());
    if !changed {
        out.note(rule, input.graph, "행위 후 재판시까지 판본이 바뀌지 않음".to_string(), &[])?;
    } else if rule.doctrine == Doctrine::Text && base.end_reason == Some(EndReason::Expired) {
        out.note(
            rule,
            input.graph,
            "행위시 판본은 유효기간 만료로 끝남 — 법령의 변경이 아니므로 행위시 판본 유지".to_string(),
            &[],
        )?;
    } else {
        let mut note = match rule.doctrine {
            Doctrine::Motive => "행위 후 재판시 전에 판본이 바뀜 — 형이 가벼워졌는지와 반성적 고려에 따른 변경인지 검토(동기설)".to_string(),
            _ => "행위 후 재판시 전에 판본이 바뀜 — 범죄가 되지 않거나 형이 가벼워졌는지 검토(변경 동기는 묻지 않음)".to_string(),
        };
        let mut extra: Vec<&str> = Vec::new();
        if at_j.is_none() {
            note.push_str(" / 재판시 판본 없음: 형이 폐지되었으면 면소 검토");
            extra.push("X:면소");
        }
        out.review(rule, note.clone());
        out.note(rule, input.graph, note, &extra)?;
        out.versions = Some(std::iter::once(base.clone()).chain(at_j).collect());
    }
    after_final(rule, input, &mut out)?;
    Ok(out)
}
