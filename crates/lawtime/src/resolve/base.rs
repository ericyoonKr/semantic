use super::{Flag, Missing, MissingReason, NotJudged, Resolution, Status, Step, StepKind};
use crate::Error;
use crate::facts::{CaseFacts, DateSpec, EventKind, Lookup};
use crate::fields::{self, ANSWER_SOURCE, AnswerRow, Row};
use crate::principles::Graph;
use crate::reftime::{self, RefChoice, RefDate, RefSource};
use crate::versions::{UnitKey, Version, VersionStore};

pub enum Base {
    Ready(Option<Version>, DateSpec),
    Stop(Resolution),
}

fn stop_missing(items: Vec<(EventKind, MissingReason)>) -> Base {
    let mut stop = Resolution::empty(Status::MissingFacts);
    stop.missing
        .extend(items.into_iter().map(|(event, reason)| Missing { event, reason, step: StepKind::RefTime }));
    Base::Stop(stop)
}

type Choice = Result<(&'static str, String, RefSource), NotJudged>;

fn choose(graph: &Graph, row: &Row, facts: &CaseFacts) -> Result<Choice, Error> {
    let Some(mut principle) = row.principle else { return Ok(Err(NotJudged::NoPrinciple)) };
    let mut choice = reftime::choose(graph, principle)?;
    if let (RefChoice::Registered { source, .. }, Some(span)) = (&choice, row.span_principle)
        && let Lookup::One(f) = facts.find(source.event())
        && matches!(f.date, DateSpec::Span { .. })
    {
        principle = span;
        choice = reftime::choose(graph, span)?;
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

type Asked = Vec<(EventKind, MissingReason)>;

fn answer_date(answer: &AnswerRow, facts: &CaseFacts) -> Result<(EventKind, DateSpec), Asked> {
    let found: Vec<(EventKind, Lookup<'_>)> =
        answer.events.iter().map(|e| (*e, facts.find(*e))).filter(|(_, l)| *l != Lookup::Absent).collect();
    match found.as_slice() {
        [(e, Lookup::One(f))] => Ok((*e, f.date)),
        [(e, _)] => Err(vec![(*e, MissingReason::Several)]),
        [] => Err(answer.events.iter().map(|e| (*e, MissingReason::Absent)).collect()),
        many => Err(many.iter().map(|(e, _)| (*e, MissingReason::Conflicting)).collect()),
    }
}

fn candidate(
    store: &dyn VersionStore,
    unit: &UnitKey,
    date: DateSpec,
    event: EventKind,
    res: &mut Resolution,
) -> Result<Base, Error> {
    let cand = match date {
        DateSpec::Point(t) => store.at(unit, t)?,
        _ => {
            let mut span = store.between(unit, date.lower(), date.upper())?;
            if span.len() > 1 {
                res.flags.push(Flag::RefTimeUndetermined {
                    note: format!("기준시 날짜 구간 안에 판본 경계가 있음 — 판본 {}개", span.len()),
                });
                res.missing.push(Missing { event, reason: MissingReason::Imprecise, step: StepKind::RefTime });
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

pub fn base(
    graph: &Graph,
    store: &dyn VersionStore,
    facts: &CaseFacts,
    unit: &UnitKey,
    res: &mut Resolution,
) -> Result<Base, Error> {
    let row = fields::of(facts.kind)?;
    let field = graph.field_label(row.field)?.to_string();
    let (date, event) = if let Some(answer) = &row.answer {
        let (event, date) = match answer_date(answer, facts) {
            Ok(x) => x,
            Err(asked) => return Ok(stop_missing(asked)),
        };
        res.trace.push(Step::RefTimeAnswer {
            kind: facts.kind,
            field,
            source: ANSWER_SOURCE.to_string(),
            basis: answer.basis.to_string(),
            event,
            date,
        });
        res.flags.push(Flag::BasisUnverified {
            source: ANSWER_SOURCE.to_string(),
            basis: format!("{} — 근거 원문 미확인", answer.basis),
        });
        (date, event)
    } else {
        let (principle, time_id, source) = match choose(graph, &row, facts)? {
            Ok(x) => x,
            Err(reason) => return Ok(Base::Stop(Resolution::empty(Status::NotJudged(reason)))),
        };
        let date = match reftime::date_of(source, facts)? {
            RefDate::Date(d) => d,
            RefDate::Absent(e) => return Ok(stop_missing(vec![(e, MissingReason::Absent)])),
            RefDate::Several(e) => return Ok(stop_missing(vec![(e, MissingReason::Several)])),
        };
        res.trace.push(Step::RefTime {
            kind: facts.kind,
            field,
            principle: graph.reference(principle)?,
            time_id,
            date,
        });
        (date, source.event())
    };
    candidate(store, unit, date, event, res)
}
