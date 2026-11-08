use chrono::NaiveDate;

use super::checks::{UNCONSTITUTIONAL_REFS, finalization_boundary, ref_boundary, unconstitutional};
use super::{Flag, Missing, MissingReason, NotJudged, Resolution, Status, Step, StepKind};
use crate::Error;
use crate::addendum::{self, COMMON_PRINCIPLE, RuleSet};
use crate::adjust::{self, AdjustTable};
use crate::facts::{CaseFacts, DateSpec, Lookup};
use crate::fields;
use crate::principles::Graph;
use crate::reftime::{self, RefChoice, RefDate, RefSource};
use crate::versions::{UnitKey, Version, VersionStore};

pub struct Engine<'a> {
    graph: &'a Graph,
    store: &'a dyn VersionStore,
    rules: &'a RuleSet,
    adjust: &'a AdjustTable,
}

enum Base {
    Ready(Option<Version>, DateSpec),
    Stop(Resolution),
}

fn missing(event: crate::facts::EventKind, reason: MissingReason) -> Resolution {
    let mut stop = Resolution::empty(Status::MissingFacts);
    stop.missing.push(Missing { event, reason, step: StepKind::RefTime });
    stop
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

    fn choose(&self, row: &fields::Row, facts: &CaseFacts) -> Result<Result<(&'static str, String, RefSource), NotJudged>, Error> {
        let Some(mut principle) = row.principle else { return Ok(Err(NotJudged::NoPrinciple)) };
        let mut choice = reftime::choose(self.graph, principle)?;
        if let (RefChoice::Registered { source, .. }, Some(span)) = (&choice, row.span_principle)
            && let Lookup::One(f) = facts.find(source.event())
                && matches!(f.date, DateSpec::Span { .. }) {
                    principle = span;
                    choice = reftime::choose(self.graph, span)?;
                }
        Ok(match choice {
            RefChoice::Registered { time_id, source } => Ok((principle, time_id, source)),
            RefChoice::NoRegisteredTime { times } => {
                Err(NotJudged::NoRegisteredTime { principle: principle.to_string(), times })
            }
            RefChoice::SeveralTimes { times } => {
                Err(NotJudged::SeveralTimes { principle: principle.to_string(), times })
            }
        })
    }

    fn base(&self, facts: &CaseFacts, unit: &UnitKey, res: &mut Resolution) -> Result<Base, Error> {
        let row = fields::of(facts.kind)?;
        let (principle, time_id, source) = match self.choose(&row, facts)? {
            Ok(x) => x,
            Err(reason) => return Ok(Base::Stop(Resolution::empty(Status::NotJudged(reason)))),
        };
        let date = match reftime::date_of(source, facts)? {
            RefDate::Date(d) => d,
            RefDate::Absent(event) => return Ok(Base::Stop(missing(event, MissingReason::Absent))),
            RefDate::Several(event) => return Ok(Base::Stop(missing(event, MissingReason::Several))),
        };
        res.trace.push(Step::RefTime {
            kind: facts.kind,
            field: self.graph.field_label(row.field)?.to_string(),
            principle: self.graph.reference(principle)?,
            time_id,
            date,
        });
        let cand = match date {
            DateSpec::Point(t) => self.store.at(unit, t)?,
            _ => {
                let mut span = self.store.between(unit, date.lower(), date.upper())?;
                if span.len() > 1 {
                    res.flags.push(Flag::RefTimeUndetermined {
                        note: format!("기준시 날짜 구간 안에 판본 경계가 있음 — 판본 {}개", span.len()),
                    });
                    res.missing.push(Missing { event: source.event(), reason: MissingReason::Imprecise, step: StepKind::RefTime });
                    let mut stop = std::mem::replace(res, Resolution::empty(Status::Undetermined));
                    stop.status = Status::Undetermined;
                    stop.versions = span;
                    return Ok(Base::Stop(stop));
                }
                span.pop()
            }
        };
        Ok(Base::Ready(cand, date))
    }

    pub fn resolve_version(&self, facts: &CaseFacts, unit: &UnitKey, as_of: NaiveDate) -> Result<Resolution, Error> {
        let mut res = Resolution::empty(Status::Decided);
        let (cand, date) = match self.base(facts, unit, &mut res)? {
            Base::Ready(c, d) => (c, d),
            Base::Stop(r) => return Ok(r.finish()),
        };
        let add = addendum::evaluate(self.rules, self.graph, facts, unit, cand, self.store)?;
        res.trace.extend(add.steps);
        res.flags.extend(add.flags);
        res.missing.extend(add.missing);
        let cand = add.candidate;
        let row = fields::of(facts.kind)?;
        let mut versions: Vec<Version> = cand.iter().cloned().collect();
        match row.adjust {
            Some(family) if add.applied => res.trace.push(Step::AdjustNone {
                family,
                as_of,
                note: "부칙 규칙이 적용되어 분야별 보정을 하지 않음(경과규정 우선)".to_string(),
            }),
            Some(family) => match self.adjust.pick(family, as_of)? {
                None => res.trace.push(Step::AdjustNone {
                    family,
                    as_of,
                    note: "판단 기준일에 유효한 보정 규칙 없음".to_string(),
                }),
                Some(rule) => {
                    let input = adjust::Input { facts, unit, base: cand.as_ref(), base_date: date, as_of, store: self.store, graph: self.graph };
                    let out = adjust::run(family, rule, &input)?;
                    res.trace.extend(out.steps);
                    res.flags.extend(out.flags);
                    res.missing.extend(out.missing);
                    if let Some(v) = out.versions {
                        versions = v;
                    }
                }
            },
            None => {}
        }
        for v in versions {
            res.push_version(v);
        }
        if !add.applied {
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
