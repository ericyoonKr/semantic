use chrono::NaiveDate;

use super::base::{Base, base};
use super::checks::{UNCONSTITUTIONAL_REFS, finalization_boundary, ref_boundary, unconstitutional};
use super::{Flag, Resolution, Status, Step};
use crate::Error;
use crate::addendum::{self, AppliedRule, COMMON_PRINCIPLE, RuleSet};
use crate::adjust::{self, AdjustTable, Family};
use crate::facts::{CaseFacts, DateSpec};
use crate::fields;
use crate::principles::Graph;
use crate::versions::{UnitKey, Version, VersionStore};

pub struct Engine<'a> {
    graph: &'a Graph,
    store: &'a dyn VersionStore,
    rules: &'a RuleSet,
    adjust: &'a AdjustTable,
}

struct Stage<'b> {
    facts: &'b CaseFacts,
    unit: &'b UnitKey,
    cand: Option<&'b Version>,
    date: DateSpec,
    as_of: NaiveDate,
    applied: Option<&'b AppliedRule>,
}

impl<'a> Engine<'a> {
    pub fn new(
        graph: &'a Graph,
        store: &'a dyn VersionStore,
        rules: &'a RuleSet,
        adjust: &'a AdjustTable,
    ) -> Result<Self, Error> {
        fields::check(graph)?;
        adjust.check(graph)?;
        graph.rule(COMMON_PRINCIPLE)?;
        for id in UNCONSTITUTIONAL_REFS {
            graph.rule(id)?;
        }
        Ok(Self { graph, store, rules, adjust })
    }

    fn adjust_stage(&self, family: Family, st: &Stage<'_>, res: &mut Resolution) -> Result<Option<Vec<Version>>, Error> {
        let rule = self.adjust.pick(family, st.as_of)?;
        if let Some(applied) = st.applied.filter(|_| family.yields_to_transition()) {
            res.trace.push(Step::AdjustYielded {
                family,
                adjust_rule_id: rule.map(|r| r.id.to_string()),
                addendum_rule_id: applied.rule_id.clone(),
                note: "경과규정이 있어 보정하지 않음".to_string(),
            });
            return Ok(None);
        }
        let Some(rule) = rule else {
            res.trace.push(Step::AdjustNone {
                family,
                as_of: st.as_of,
                note: "판단 기준일에 유효한 보정 규칙 없음".to_string(),
            });
            return Ok(None);
        };
        let input = adjust::Input {
            facts: st.facts,
            unit: st.unit,
            base: st.cand,
            base_date: st.date,
            as_of: st.as_of,
            store: self.store,
            graph: self.graph,
        };
        let out = adjust::run(family, rule, &input)?;
        res.trace.extend(out.steps);
        res.flags.extend(out.flags);
        res.missing.extend(out.missing);
        Ok(out.versions)
    }

    pub fn resolve_version(&self, facts: &CaseFacts, unit: &UnitKey, as_of: NaiveDate) -> Result<Resolution, Error> {
        let mut res = Resolution::empty(Status::Decided);
        let (cand, date) = match base(self.graph, self.store, facts, unit, &mut res)? {
            Base::Ready(c, d) => (c, d),
            Base::Stop(r) => return Ok(r.finish()),
        };
        let add = addendum::evaluate(self.rules, self.graph, facts, unit, cand, self.store)?;
        res.trace.extend(add.steps);
        res.flags.extend(add.flags);
        res.missing.extend(add.missing);
        let cand = add.candidate;
        let applied = add.applied;
        let mut versions: Vec<Version> = cand.iter().cloned().collect();
        if let Some(family) = fields::of(facts.kind)?.adjust {
            let st = Stage { facts, unit, cand: cand.as_ref(), date, as_of, applied: applied.as_ref() };
            if let Some(v) = self.adjust_stage(family, &st, &mut res)? {
                versions = v;
            }
        }
        for v in versions {
            res.push_version(v);
        }
        if applied.is_none() {
            ref_boundary(self.store, unit, date, cand.as_ref(), &mut res)?;
        }
        finalization_boundary(self.store, unit, facts, &mut res)?;
        unconstitutional(self.graph, &mut res)?;
        if res.versions.is_empty() {
            res.flags.push(Flag::NoVersion { note: "기준시에 적용할 판본이 없음".to_string() });
        }
        Ok(res.finish())
    }
}
