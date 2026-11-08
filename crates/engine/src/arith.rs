pub mod compute;
pub mod extract;
pub mod prompt;

use std::collections::HashMap;

use checks::holidays::HolidayData;
use checks::periods::{Holidays, StartKnown};
use chrono::NaiveDate;
use futures::future::join_all;
use tokio_util::sync::CancellationToken;

use crate::deps::{AuthoritySearch, EngineDeps, FactProvider, LeafCache};
use crate::graph::ClaimGraph;
use crate::leaf::ask::Caller;
use crate::leaf::settle::from_failure;
use crate::period::{HolidayNote, PeriodReport};
use crate::schema::{ArithSpec, FailureReason, LeafResult, NodeId, PeriodRule};
use crate::slices::{NO_FACTS, Prepared};
use compute::{Dates, finish};
use extract::{DateAnswer, Found, resolve, resolve_start};

pub const RESTRICTIVE_OPEN: &str = "행정기본법 제6조 제2항의 기간(권익을 제한하거나 의무를 부과하는 기간)은 계산 기준을 아직 넣지 않아 판정하지 않음";

pub type PeriodResults = (HashMap<NodeId, LeafResult>, Vec<PeriodReport>);

struct Ctx<'a, F> {
    caller: Caller,
    facts: &'a F,
    session: &'a str,
    today: NaiveDate,
    table: Holidays,
    note: HolidayNote,
}

fn note_of(data: &HolidayData) -> HolidayNote {
    match data {
        HolidayData::Loaded(_) => HolidayNote::Applied,
        HolidayData::NotSet => HolidayNote::NotSet,
        HolidayData::Failed(why) => HolidayNote::Failed(why.clone()),
    }
}

pub async fn judge_periods<F, A, C>(
    prepared: Vec<Prepared>,
    subtree: &ClaimGraph,
    deps: &EngineDeps<F, A, C>,
    session: &str,
    today: NaiveDate,
    cancel: &CancellationToken,
) -> PeriodResults
where
    F: FactProvider,
    A: AuthoritySearch,
    C: LeafCache,
{
    let ctx = Ctx {
        caller: Caller::new(deps, cancel),
        facts: &deps.facts,
        session,
        today,
        table: deps.settings.holidays.table(),
        note: note_of(&deps.settings.holidays),
    };
    let jobs = prepared.into_iter().filter_map(|p| {
        let node = subtree.node(p.node_id())?;
        let spec = node.arithmetic.clone()?;
        Some(one(p, spec, node.period_rule, &ctx))
    });
    let mut results = HashMap::new();
    let mut reports = Vec::new();
    for (id, result, report) in join_all(jobs).await {
        results.insert(id, result);
        reports.push(report);
    }
    reports.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    (results, reports)
}

async fn one<F: FactProvider>(
    prepared: Prepared,
    spec: ArithSpec,
    rule: Option<PeriodRule>,
    ctx: &Ctx<'_, F>,
) -> (NodeId, LeafResult, PeriodReport) {
    let id = prepared.node_id().clone();
    let report = PeriodReport::blank(id.clone(), &spec, ctx.note.clone());
    if rule == Some(PeriodRule::AdministrativeRestrictive) {
        let failure = FailureReason::NoEvidence(RESTRICTIVE_OPEN.to_string());
        return (id, LeafResult::Unknown { failure }, report);
    }
    let req = match prepared {
        Prepared::Ready(req) => req,
        Prepared::NoEvidence { .. } => {
            let failure = FailureReason::NoEvidence(NO_FACTS.to_string());
            return (id, LeafResult::Unknown { failure }, report);
        }
        Prepared::Failed { failure, .. } => return (id, LeafResult::Unknown { failure }, report),
    };
    let user = prompt::user_message(&req, &report.from.name, &report.to.name, report.length);
    let answer = match ctx.caller.call::<DateAnswer>(prompt::SYSTEM, &user).await {
        Ok(answer) => answer,
        Err(failure) => return (id, from_failure(failure).result, report),
    };
    let from = resolve(answer.item(&report.from.name), ctx.facts, ctx.session, &req.facts).await;
    let to = resolve(answer.item(&report.to.name), ctx.facts, ctx.session, &req.facts).await;
    let (start, start_quote) = match &from {
        Found::Date { date, .. } => {
            resolve_start(answer.start.as_ref(), *date, ctx.facts, ctx.session, &req.facts).await
        }
        _ => (StartKnown::Undetermined, None),
    };
    let dates = Dates {
        from,
        to,
        start,
        start_quote,
        today: ctx.today,
    };
    let (result, report) = finish(report, dates, &ctx.table);
    (id, result, report)
}
