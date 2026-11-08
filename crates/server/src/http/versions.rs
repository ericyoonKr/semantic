use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use chrono::{DateTime, Local, NaiveDate, Utc};
use lawtime::addendum::RuleSet;
use lawtime::citation::{Bound, CitationResolution, CiteType, parse_citations, resolve_citation};
use lawtime::facts::{CaseFacts, CaseKind, DateSpec, EventKind, Fact, ProcState};
use lawtime::lanes::{LaneOutcome, compare};
use lawtime::resolve::{Engine, Resolution};
use lawtime::versions::UnitKey;
use lawtime::versions::pg::{LawLookup, LawRef, PgStore, article_of, find_law};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::ApiError;
use crate::state::AppState;
use crate::versions::LawtimeParts;

const MAX_UNITS: usize = 50;
const MAX_TEXT_CHARS: usize = 200_000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactIn {
    pub event: EventKind,
    pub on: Option<NaiveDate>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    #[serde(default)]
    pub continuing: bool,
}

#[derive(Debug, Deserialize)]
pub struct ResolveRequest {
    pub law: String,
    pub unit: String,
    pub kind: CaseKind,
    #[serde(default)]
    pub facts: Vec<FactIn>,
    pub state: Option<ProcState>,
    pub as_of: Option<NaiveDate>,
    pub known_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct ResolveReply {
    pub law: LawRef,
    pub unit: UnitKey,
    pub as_of: NaiveDate,
    pub known_at: Option<DateTime<Utc>>,
    pub data_from: Option<NaiveDate>,
    pub resolution: Resolution,
}

#[derive(Debug, Deserialize)]
pub struct CitationsRequest {
    pub text: String,
    pub judgment_date: NaiveDate,
    pub known_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CiteItem {
    pub law_name: String,
    pub old: bool,
    pub cite_type: CiteType,
    pub bound: Bound,
    pub article: String,
    pub start: usize,
    pub end: usize,
    pub law: Option<LawRef>,
    pub unit: Option<UnitKey>,
    pub data_from: Option<NaiveDate>,
    pub resolution: Option<CitationResolution>,
    pub unresolved: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CitationsReply {
    pub judgment_date: NaiveDate,
    pub known_at: Option<DateTime<Utc>>,
    pub items: Vec<CiteItem>,
    pub truncated: bool,
}

#[derive(Debug, Deserialize)]
pub struct TraceRequest {
    pub text: String,
    pub judgment_date: NaiveDate,
    pub kind: CaseKind,
    #[serde(default)]
    pub facts: Vec<FactIn>,
    pub state: Option<ProcState>,
    pub known_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct LaneRow {
    pub answer: CiteItem,
    pub prediction: Option<Resolution>,
    pub outcome: LaneOutcome,
}

#[derive(Debug, Serialize)]
pub struct TraceReply {
    pub judgment_date: NaiveDate,
    pub known_at: Option<DateTime<Utc>>,
    pub rows: Vec<LaneRow>,
    pub counts: BTreeMap<String, usize>,
    pub truncated: bool,
}

fn json_body<T>(body: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    body.map(|Json(v)| v)
        .map_err(|e| ApiError::bad_request(format!("인자 오류: {}", e.body_text())))
}

fn failure(reason: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, format!("판본 결정 실패: {}", reason.into()))
}

fn lawtime_error(e: lawtime::Error) -> ApiError {
    match e {
        lawtime::Error::Db(err) => {
            tracing::error!(error = %err, "version database query failed");
            failure("법령 DB 오류")
        }
        other => {
            tracing::error!(error = %other, "version resolution failed");
            failure(format!("판본 자료를 처리하지 못함({other})"))
        }
    }
}

fn pool(state: &AppState) -> Result<&PgPool, ApiError> {
    state
        .corpus
        .as_ref()
        .map(|c| c.pool())
        .ok_or_else(|| failure("법령 DB가 설정되지 않았습니다"))
}

fn parts(state: &AppState) -> Result<&LawtimeParts, ApiError> {
    state
        .lawtime
        .as_deref()
        .ok_or_else(|| failure("시점원칙 그래프가 설정되지 않았습니다"))
}

async fn within<T>(state: &AppState, work: impl Future<Output = Result<T, ApiError>>) -> Result<Json<T>, ApiError> {
    match tokio::time::timeout(state.cfg.corpus.search_deadline, work).await {
        Ok(r) => r.map(Json),
        Err(_) => Err(ApiError::timeout()),
    }
}

fn case_facts(kind: CaseKind, input: Vec<FactIn>, state: Option<ProcState>) -> Result<CaseFacts, ApiError> {
    let mut facts = CaseFacts::new(kind);
    facts.state = state;
    for f in input {
        let date = match (f.on, f.from, f.to) {
            (Some(d), None, None) if !f.continuing => DateSpec::Point(d),
            (None, Some(from), to) => {
                if to.is_some_and(|t| t < from) {
                    return Err(ApiError::bad_request("인자 오류: 날짜 구간의 끝이 시작보다 앞섭니다"));
                }
                if f.continuing { DateSpec::Span { from, to } } else { DateSpec::Approx { from, to } }
            }
            _ => {
                return Err(ApiError::bad_request(
                    "인자 오류: 사실의 날짜는 on 하나, 또는 from(과 to)로 줍니다",
                ));
            }
        };
        facts.facts.push(Fact { event: f.event, date, source: None });
    }
    Ok(facts)
}

fn unit_article(cited: &str) -> Result<String, ApiError> {
    article_of(cited)
        .map_err(lawtime_error)?
        .ok_or_else(|| ApiError::bad_request("인자 오류: 조문은 「제N조」 또는 「제N조의M」로 줍니다(부칙·별표는 받지 않음)"))
}

fn law_problem(name: &str, found: &LawLookup) -> Option<String> {
    match found {
        LawLookup::One(_) => None,
        LawLookup::NotFound => Some(format!("법령 DB에서 「{name}」을 찾지 못함")),
        LawLookup::Several(laws) => Some(format!(
            "이름이 같은 법령이 여럿 — 법령ID로 줄 것: {}",
            laws.iter().map(|l| format!("{}({})", l.law_id, l.name)).collect::<Vec<_>>().join(", ")
        )),
        LawLookup::NotStatute(_) => Some(format!("「{name}」은 행정규칙이라 판본 결정 대상이 아님")),
    }
}

fn predict(
    parts: &LawtimeParts,
    store: &PgStore,
    facts: &CaseFacts,
    unit: &UnitKey,
    as_of: NaiveDate,
) -> Result<Resolution, ApiError> {
    let rules = RuleSet::default();
    let engine = Engine::new(&parts.graph, store, &rules, &parts.adjust).map_err(lawtime_error)?;
    let res = engine.resolve_version(facts, unit, as_of).map_err(lawtime_error)?;
    Ok(store.check_resolution(unit, facts, res, as_of))
}

pub async fn resolve(
    State(state): State<AppState>,
    body: Result<Json<ResolveRequest>, JsonRejection>,
) -> Result<Json<ResolveReply>, ApiError> {
    let req = json_body(body)?;
    let article = unit_article(&req.unit)?;
    let facts = case_facts(req.kind, req.facts, req.state)?;
    let as_of = req.as_of.unwrap_or_else(|| Local::now().date_naive());
    let parts = parts(&state)?;
    let pool = pool(&state)?;
    within(&state, async {
        let found = find_law(pool, &req.law).await.map_err(lawtime_error)?;
        let law = match found {
            LawLookup::One(l) => l,
            other => return Err(ApiError::bad_request(law_problem(req.law.trim(), &other).unwrap_or_default())),
        };
        let unit = UnitKey::article(&law.law_id, &article);
        let store = PgStore::load(pool, std::slice::from_ref(&law), std::slice::from_ref(&unit), req.known_at)
            .await
            .map_err(lawtime_error)?;
        let resolution = predict(parts, &store, &facts, &unit, as_of)?;
        Ok(ResolveReply {
            data_from: store.first_effective(&law.law_id),
            law,
            unit,
            as_of,
            known_at: req.known_at,
            resolution,
        })
    })
    .await
}

fn check_text(text: &str) -> Result<(), ApiError> {
    if text.trim().is_empty() {
        return Err(ApiError::bad_request("인자 오류: text가 비어 있습니다"));
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(ApiError::bad_request(format!("인자 오류: text는 {MAX_TEXT_CHARS}자까지")));
    }
    Ok(())
}

fn blank(c: &lawtime::citation::Citation, article: &str) -> CiteItem {
    CiteItem {
        law_name: c.law_name.clone(),
        old: c.old,
        cite_type: c.cite_type(),
        bound: c.bound.clone(),
        article: article.to_string(),
        start: c.start,
        end: c.end,
        law: None,
        unit: None,
        data_from: None,
        resolution: None,
        unresolved: None,
    }
}

async fn answer_lane(
    pool: &PgPool,
    text: &str,
    judgment_date: NaiveDate,
    known_at: Option<DateTime<Utc>>,
) -> Result<(PgStore, Vec<CiteItem>, bool), ApiError> {
    let cites = parse_citations(text).map_err(lawtime_error)?;
    let mut laws: BTreeMap<String, LawLookup> = BTreeMap::new();
    let mut work: Vec<(CiteItem, Option<lawtime::citation::Citation>)> = Vec::new();
    let mut units: Vec<UnitKey> = Vec::new();
    let mut truncated = false;
    for c in cites {
        let articles = if c.articles.is_empty() { vec![String::new()] } else { c.articles.clone() };
        for a in articles {
            let mut item = blank(&c, &a);
            let Some(article) = article_of(&a).map_err(lawtime_error)? else {
                item.unresolved = Some("조 단위가 아닌 인용(부칙·별표·조문 없음)은 판본 자료에 없음".to_string());
                work.push((item, None));
                continue;
            };
            if c.old && c.bound == Bound::Unmarked {
                item.unresolved = Some("「구」 표시가 있으나 판본 경계가 없음 — 판본을 정하지 않음".to_string());
                work.push((item, None));
                continue;
            }
            if !laws.contains_key(&c.law_name) {
                let found = find_law(pool, &c.law_name).await.map_err(lawtime_error)?;
                laws.insert(c.law_name.clone(), found);
            }
            let found = laws.get(&c.law_name).cloned().unwrap_or(LawLookup::NotFound);
            let LawLookup::One(law) = &found else {
                item.unresolved = law_problem(&c.law_name, &found);
                work.push((item, None));
                continue;
            };
            let unit = UnitKey::article(&law.law_id, &article);
            if !units.contains(&unit) {
                if units.len() >= MAX_UNITS {
                    truncated = true;
                    item.unresolved = Some(format!("한 번에 {MAX_UNITS}개 조문까지만 해소함"));
                    work.push((item, None));
                    continue;
                }
                units.push(unit.clone());
            }
            item.law = Some(law.clone());
            item.unit = Some(unit);
            work.push((item, Some(c.clone())));
        }
    }
    let found_laws: Vec<LawRef> = laws
        .values()
        .filter_map(|l| match l {
            LawLookup::One(law) => Some(law.clone()),
            _ => None,
        })
        .collect();
    let store = PgStore::load(pool, &found_laws, &units, known_at).await.map_err(lawtime_error)?;
    let mut items = Vec::with_capacity(work.len());
    for (mut item, cite) in work {
        if let (Some(c), Some(unit)) = (cite, item.unit.clone()) {
            let dates: Vec<NaiveDate> = c.bound.promuls().iter().map(|p| p.date).collect();
            let res = resolve_citation(&store, &c, &unit, judgment_date).map_err(lawtime_error)?;
            item.data_from = store.first_effective(&unit.law_id);
            item.resolution = Some(store.check_citation(res, &dates));
        }
        items.push(item);
    }
    Ok((store, items, truncated))
}

pub async fn citations(
    State(state): State<AppState>,
    body: Result<Json<CitationsRequest>, JsonRejection>,
) -> Result<Json<CitationsReply>, ApiError> {
    let req = json_body(body)?;
    check_text(&req.text)?;
    let pool = pool(&state)?;
    within(&state, async {
        let (_, items, truncated) = answer_lane(pool, &req.text, req.judgment_date, req.known_at).await?;
        Ok(CitationsReply { judgment_date: req.judgment_date, known_at: req.known_at, items, truncated })
    })
    .await
}

fn outcome_name(o: LaneOutcome) -> String {
    match o {
        LaneOutcome::Same => "same",
        LaneOutcome::AmongSeveral => "among_several",
        LaneOutcome::Different => "different",
        LaneOutcome::NoAnswer => "no_answer",
        LaneOutcome::NoPrediction => "no_prediction",
    }
    .to_string()
}

pub async fn trace(
    State(state): State<AppState>,
    body: Result<Json<TraceRequest>, JsonRejection>,
) -> Result<Json<TraceReply>, ApiError> {
    let req = json_body(body)?;
    check_text(&req.text)?;
    let facts = case_facts(req.kind, req.facts, req.state)?;
    let parts = parts(&state)?;
    let pool = pool(&state)?;
    within(&state, async {
        let (store, items, truncated) = answer_lane(pool, &req.text, req.judgment_date, req.known_at).await?;
        let mut rows = Vec::with_capacity(items.len());
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for answer in items {
            let unit = answer.resolution.as_ref().map(|r| r.unit.clone()).or_else(|| answer.unit.clone());
            let (prediction, outcome) = match (&answer.resolution, unit) {
                (Some(r), Some(unit)) => {
                    let p = predict(parts, &store, &facts, &unit, req.judgment_date)?;
                    let o = compare(r, &p);
                    (Some(p), o)
                }
                _ => (None, LaneOutcome::NoAnswer),
            };
            *counts.entry(outcome_name(outcome)).or_default() += 1;
            rows.push(LaneRow { answer, prediction, outcome });
        }
        Ok(TraceReply { judgment_date: req.judgment_date, known_at: req.known_at, rows, counts, truncated })
    })
    .await
}
