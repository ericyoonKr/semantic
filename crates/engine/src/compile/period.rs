use std::collections::HashMap;

use checks::dates::to_ordinal;
use z3::ast::{Bool, Int};

use crate::period::PeriodReport;
use crate::schema::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeriodFacts {
    pub to: i64,
    pub lo: i64,
    pub hi: Option<i64>,
}

pub fn period_facts(reports: &[PeriodReport]) -> HashMap<NodeId, PeriodFacts> {
    reports
        .iter()
        .filter(|r| r.value.is_some())
        .filter_map(|r| {
            Some((
                r.node_id.clone(),
                PeriodFacts {
                    to: to_ordinal(r.to.date?),
                    lo: to_ordinal(r.lo?),
                    hi: r.hi.map(to_ordinal),
                },
            ))
        })
        .collect()
}

pub(super) fn encode(var: &Bool, id: &NodeId, facts: &PeriodFacts) -> (Bool, Bool) {
    let to = Int::new_const(format!("{id}::제소일"));
    let last = Int::new_const(format!("{id}::말일"));
    let definition = var.iff(to.le(&last));
    let mut known = vec![to.eq(Int::from_i64(facts.to)), last.ge(Int::from_i64(facts.lo))];
    if let Some(hi) = facts.hi {
        known.push(last.le(Int::from_i64(hi)));
    }
    (definition, Bool::and(&known))
}
