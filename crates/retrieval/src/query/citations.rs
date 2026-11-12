use chrono::NaiveDate;
use sqlx::PgPool;

use crate::error::RetrievalError;
use crate::model::{CitationItem, Citations, CitationsBy, CitedUnit, CitingCase};
use crate::query::cases::cited::article_keys_of;
use crate::query::direct::article_of;
use crate::query::laws::resolve_law;
use crate::query::terms::{compact, like_prefix};

const ACTIVE_RUN: &str =
    "SELECT run_id, version FROM extract_run WHERE extractor = 'citation' AND is_active ORDER BY run_id DESC LIMIT 1";

const CITED: &str = "SELECT l.name, dc.law_id, dc.unit_path, dc.article_key, dc.article_version_id, dc.relation, dc.raw \
     FROM decision_citation dc \
     LEFT JOIN law l ON l.law_id = dc.law_id \
     WHERE dc.run_id = $1 AND dc.decision_id = $2 \
     ORDER BY dc.section_id, dc.char_start, dc.citation_id LIMIT $3";

const CITING: &str = "SELECT d.decision_id, c.name, d.case_no_display, d.decided_on, \
       COALESCE(dc.unit_path, ''), dc.relation, dc.article_version_id \
     FROM decision_citation dc \
     JOIN decision d ON d.decision_id = dc.decision_id \
     JOIN court c ON c.court_id = d.court_id \
     WHERE dc.run_id = $1 AND dc.law_id = $2 AND dc.article_key = ANY($3::text[]) \
       AND ($4 = '' OR dc.unit_path = $5 OR dc.unit_path LIKE $6 OR dc.unit_path LIKE $7) \
     ORDER BY d.decided_on DESC, d.decision_id, dc.citation_id LIMIT $8";

type CitedRow = (Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, String, String);

type CitingRow = (i64, String, String, NaiveDate, String, String, Option<String>);

async fn cited(pool: &PgPool, run_id: i32, decision_id: i64, limit: i64) -> Result<Vec<CitationItem>, RetrievalError> {
    let rows: Vec<CitedRow> = sqlx::query_as(CITED).bind(run_id).bind(decision_id).bind(limit).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(law_name, law_id, unit_key, article_key, article_version_id, relation, raw)| {
            CitationItem::Cited(CitedUnit { law_name, law_id, unit_key, article_key, article_version_id, relation, raw })
        })
        .collect())
}

async fn citing(pool: &PgPool, run_id: i32, law: &str, unit: &str, limit: i64) -> Result<Vec<CitationItem>, RetrievalError> {
    let unit = compact(unit);
    if law.trim().is_empty() || unit.is_empty() {
        return Err(RetrievalError::Argument("law and unit"));
    }
    let article = article_of(&unit).ok_or(RetrievalError::Argument("unit"))?;
    let rest = unit.strip_prefix(article.as_str()).unwrap_or_default();
    let Some((law_id, _, _)) = resolve_law(pool, law).await? else {
        return Ok(Vec::new());
    };
    let keys = article_keys_of(pool, &law_id, &article).await?;
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let prefix = like_prefix(&unit);
    let rows: Vec<CitingRow> = sqlx::query_as(CITING)
        .bind(run_id)
        .bind(&law_id)
        .bind(&keys)
        .bind(rest)
        .bind(&unit)
        .bind(format!("{prefix}제%"))
        .bind(format!("{prefix}_목%"))
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(decision_id, court, case_no, decided_on, unit_key, relation, article_version_id)| {
            CitationItem::Citing(CitingCase { decision_id, court, case_no, decided_on, unit_key, relation, article_version_id })
        })
        .collect())
}

pub async fn citations(pool: &PgPool, by: &CitationsBy, limit: i64) -> Result<Citations, RetrievalError> {
    let run: Option<(i32, String)> = sqlx::query_as(ACTIVE_RUN).fetch_optional(pool).await?;
    let Some((run_id, version)) = run else {
        tracing::warn!("no active citation extract run; citations are empty");
        return Ok(Citations { items: Vec::new(), extract_run: None });
    };
    let items = match by {
        CitationsBy::Decision(id) => cited(pool, run_id, *id, limit).await?,
        CitationsBy::LawUnit { law, unit } => citing(pool, run_id, law, unit, limit).await?,
    };
    Ok(Citations { items, extract_run: Some(format!("{run_id}:{version}")) })
}
