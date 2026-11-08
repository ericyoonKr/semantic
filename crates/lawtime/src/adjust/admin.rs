use super::{AdjustResult, AdjustRule, Input, changes, differs};
use crate::Error;
use crate::facts::EventKind;
use crate::resolve::{Missing, MissingReason, StepKind};
use crate::versions::Version;

fn window_start(input: &Input<'_>, out: &mut AdjustResult, event: EventKind) -> Option<chrono::NaiveDate> {
    let t = input.base_date.upper();
    if t.is_none() {
        out.missing.push(Missing { event, reason: MissingReason::Imprecise, step: StepKind::Adjust });
    }
    t
}

fn changed_between(
    input: &Input<'_>,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> Result<(bool, Vec<Version>, Option<Version>), Error> {
    let list = changes(input.store, input.unit, from, to)?;
    let at_from = input.store.at(input.unit, from)?;
    let at_to = input.store.at(input.unit, to)?;
    let changed = !list.is_empty() || differs(at_from.as_ref(), at_to.as_ref());
    Ok((changed, list, at_to))
}

pub fn sanction(rule: &AdjustRule, input: &Input<'_>) -> Result<AdjustResult, Error> {
    let mut out = AdjustResult::default();
    let Some(t) = window_start(input, &mut out, EventKind::Act) else { return Ok(out) };
    let Some(d) = out.point(input.facts, EventKind::Disposition, true) else { return Ok(out) };
    let (changed, list, at_d) = changed_between(input, t, d)?;
    if !changed {
        out.note(rule, input.graph, "위반행위 후 처분시까지 판본이 바뀌지 않음".to_string(), &[])?;
        return Ok(out);
    }
    let pivot = rule.change_from;
    let before = |v: &Version| pivot.is_some_and(|p| v.promul_date < p && v.valid_period.from < p);
    if !list.is_empty() && list.iter().all(before) {
        out.note(
            rule,
            input.graph,
            "판본 변경이 제14조 제3항 단서의 적용 시점 전 — 위반행위 당시 판본 유지".to_string(),
            &[],
        )?;
        return Ok(out);
    }
    let note = "위반행위 후 처분 전에 판본이 바뀜 — 위반행위가 아니게 되었거나 제재 기준이 가벼워졌는지 검토".to_string();
    out.review(rule, note.clone());
    out.note(rule, input.graph, note, &[])?;
    out.versions = Some(input.base.cloned().into_iter().chain(at_d).collect());
    Ok(out)
}

pub fn fine(rule: &AdjustRule, input: &Input<'_>) -> Result<AdjustResult, Error> {
    let mut out = AdjustResult::default();
    let Some(t) = window_start(input, &mut out, EventKind::Act) else { return Ok(out) };
    let finalized = out.point(input.facts, EventKind::Finalized, false);
    let end = finalized.unwrap_or(input.as_of);
    let (changed, _, at_end) = changed_between(input, t, end)?;
    if changed {
        let note = "질서위반행위 후 판본이 바뀜 — 질서위반행위가 아니게 되었거나 과태료가 가벼워졌는지 검토".to_string();
        out.review(rule, note.clone());
        out.note(rule, input.graph, note, &[])?;
        out.versions = Some(input.base.cloned().into_iter().chain(at_end).collect());
    } else {
        out.note(rule, input.graph, "질서위반행위 후 판본이 바뀌지 않음".to_string(), &[])?;
    }
    if let Some(f) = finalized
        && changed_between(input, f, input.as_of)?.0 {
            let note = "확정 후 판본이 바뀜 — 질서위반행위가 아니게 되었으면 징수·집행 면제 검토".to_string();
            out.review(rule, note.clone());
            out.note(rule, input.graph, note, &["X:과태료확정후면제"])?;
        }
    Ok(out)
}

pub fn application(rule: &AdjustRule, input: &Input<'_>) -> Result<AdjustResult, Error> {
    let mut out = AdjustResult::default();
    let Some(d) = window_start(input, &mut out, EventKind::Disposition) else { return Ok(out) };
    let Some(a) = out.point(input.facts, EventKind::ApplicationFiled, true) else { return Ok(out) };
    let (changed, _, _) = changed_between(input, a, d)?;
    if !changed {
        out.note(rule, input.graph, "신청 후 처분시까지 판본이 바뀌지 않음".to_string(), &[])?;
        return Ok(out);
    }
    let days = (d - a).num_days();
    let at_a = input.store.at(input.unit, a)?;
    let mut note = format!("신청 후 처분 전 개정 시행, 처리기간 {days}일 → 정당한 이유 없는 지연 여부 검토");
    if at_a.is_none() {
        note.push_str(" / 신청시 판본 없음");
    }
    out.review(rule, note.clone());
    out.note(rule, input.graph, note, &[])?;
    out.versions = Some(input.base.cloned().into_iter().chain(at_a).collect());
    Ok(out)
}
