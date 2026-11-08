use serde::Serialize;

use crate::Error;
use crate::facts::{CaseFacts, DateSpec, EventKind, Lookup};
use crate::principles::Graph;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RefSource {
    Event(EventKind),
    EventEnd(EventKind),
    EventNextDay(EventKind),
}

pub const REGISTRY: [(&str, Option<RefSource>); 18] = [
    ("T:공포일", None),
    ("T:시행일", None),
    ("T:행위시", Some(RefSource::Event(EventKind::Act))),
    ("T:실행종료", Some(RefSource::EventEnd(EventKind::Act))),
    ("T:재판시", Some(RefSource::Event(EventKind::Judgment))),
    ("T:확정후", Some(RefSource::Event(EventKind::Finalized))),
    ("T:처분시", Some(RefSource::Event(EventKind::Disposition))),
    ("T:완성종결", Some(RefSource::Event(EventKind::Completed))),
    ("T:진행중", Some(RefSource::Event(EventKind::ProcedureAct))),
    ("T:납세의무성립", Some(RefSource::Event(EventKind::TaxLiability))),
    ("T:사망시", Some(RefSource::Event(EventKind::Death))),
    ("T:공소제기", Some(RefSource::Event(EventKind::Indictment))),
    ("T:변론종결", Some(RefSource::Event(EventKind::FirstInstanceClosed))),
    ("T:판단당시", None),
    ("T:위헌결정일", Some(RefSource::Event(EventKind::UnconstitutionalDecision))),
    ("T:종전합헌결정다음날", Some(RefSource::EventNextDay(EventKind::PriorConstitutionalDecision))),
    ("T:판례변경선고", Some(RefSource::Event(EventKind::PrecedentChange))),
    ("T:헌법불합치선고일", Some(RefSource::Event(EventKind::NonconformityDecision))),
];

pub fn registered(time_id: &str) -> Option<RefSource> {
    REGISTRY.iter().find(|(id, _)| *id == time_id).and_then(|(_, s)| *s)
}

impl RefSource {
    pub fn event(self) -> EventKind {
        match self {
            Self::Event(e) | Self::EventEnd(e) | Self::EventNextDay(e) => e,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum RefChoice {
    Registered { time_id: String, source: RefSource },
    NoRegisteredTime { times: Vec<String> },
    SeveralTimes { times: Vec<String> },
}

pub fn choose(graph: &Graph, principle_id: &str) -> Result<RefChoice, Error> {
    let node = graph.rule(principle_id)?;
    let hits: Vec<(&String, RefSource)> = node
        .times
        .iter()
        .filter_map(|t| registered(t).map(|s| (t, s)))
        .collect();
    Ok(match hits.as_slice() {
        [(t, s)] => RefChoice::Registered { time_id: (*t).clone(), source: *s },
        [] => RefChoice::NoRegisteredTime { times: node.times.clone() },
        _ => RefChoice::SeveralTimes { times: hits.iter().map(|(t, _)| (*t).clone()).collect() },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RefDate {
    Date(DateSpec),
    Absent(EventKind),
    Several(EventKind),
}

pub fn date_of(source: RefSource, facts: &CaseFacts) -> Result<RefDate, Error> {
    let event = source.event();
    let fact = match facts.find(event) {
        Lookup::Absent => return Ok(RefDate::Absent(event)),
        Lookup::Several => return Ok(RefDate::Several(event)),
        Lookup::One(f) => f,
    };
    let date = match (source, fact.date) {
        (RefSource::Event(_), d) => d,
        (RefSource::EventEnd(_), DateSpec::Span { from, to: None }) => {
            DateSpec::Approx { from, to: None }
        }
        (RefSource::EventEnd(_), DateSpec::Span { to: Some(t), .. }) => DateSpec::Point(t),
        (RefSource::EventEnd(_), d) => d,
        (RefSource::EventNextDay(_), DateSpec::Point(d)) => {
            DateSpec::Point(crate::period::next_day(d)?)
        }
        (RefSource::EventNextDay(_), d) => {
            let from = crate::period::next_day(d.lower())?;
            let to = d.upper().map(crate::period::next_day).transpose()?;
            DateSpec::Approx { from, to }
        }
    };
    Ok(RefDate::Date(date))
}
