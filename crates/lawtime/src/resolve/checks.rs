use chrono::NaiveDate;

use super::{Flag, Resolution, Step};
use crate::Error;
use crate::facts::{CaseFacts, DateSpec, EventKind, Lookup};
use crate::period::prev_day;
use crate::principles::Graph;
use crate::versions::{EndReason, UnitKey, Version, VersionStore};

pub const UNCONSTITUTIONAL_REFS: [&str; 3] = ["P:위헌결정장래효", "X:형벌조항소급실효", "X:위헌소급효제한"];

pub fn ref_boundary(
    store: &dyn VersionStore,
    unit: &UnitKey,
    date: DateSpec,
    cand: Option<&Version>,
    res: &mut Resolution,
) -> Result<(), Error> {
    let (Some(t), Some(c)) = (date.point(), cand) else { return Ok(()) };
    if c.valid_period.from != t {
        return Ok(());
    }
    let prev = store.at(unit, prev_day(t)?)?;
    res.flags.push(Flag::Boundary {
        date: t,
        note: "기준시가 판본 시행일과 같은 날 — 양쪽 판본을 함께 냄".to_string(),
    });
    if let Some(p) = prev {
        res.push_version(p);
    }
    Ok(())
}

pub fn finalization_boundary(
    store: &dyn VersionStore,
    unit: &UnitKey,
    facts: &CaseFacts,
    res: &mut Resolution,
) -> Result<(), Error> {
    let Lookup::One(f) = facts.find(EventKind::Finalized) else { return Ok(()) };
    let Some(d) = f.date.point() else { return Ok(()) };
    let same_day: Vec<Version> = store
        .between(unit, d, None)?
        .into_iter()
        .filter(|v| v.promul_date == d)
        .collect();
    if same_day.is_empty() {
        return Ok(());
    }
    res.flags.push(Flag::Boundary {
        date: d,
        note: "판본 공포일이 재판 확정일과 같은 날 — 양쪽 판본을 함께 냄".to_string(),
    });
    for v in same_day {
        res.push_version(v);
    }
    Ok(())
}

pub fn unconstitutional(graph: &Graph, res: &mut Resolution) -> Result<(), Error> {
    let ended: Vec<(UnitKey, Option<NaiveDate>)> = res
        .versions
        .iter()
        .filter(|v| v.end_reason == Some(EndReason::Unconstitutional))
        .map(|v| (v.unit.clone(), v.valid_period.until))
        .collect();
    if ended.is_empty() {
        return Ok(());
    }
    let refs = UNCONSTITUTIONAL_REFS
        .iter()
        .map(|id| graph.reference(id))
        .collect::<Result<Vec<_>, Error>>()?;
    res.trace.push(Step::Check {
        id: "CHK-위헌결정".to_string(),
        note: "위헌결정으로 끝난 판본 — 소급 범위(형벌 조항, 종전 합헌결정, 소급효 제한) 확인 필요".to_string(),
        refs,
    });
    for (unit, until) in ended {
        res.flags.push(Flag::UnconstitutionalScope { unit, ended: until });
    }
    Ok(())
}
