use std::collections::HashMap;

use chrono::NaiveDate;
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgPool, Row};

use crate::error::RetrievalError;
use crate::model::{LawHit, LawQuery, LawUnit, LawVersion};
use crate::query::annex::{AnnexName, annex_label};
use crate::query::direct::{Direct, Target};
use crate::query::fusion::CANDIDATES;
use crate::query::terms::{Terms, compact};
use crate::query::weights::LawWeights;

pub const FIRST_STEP_TIMEOUT_MS: u64 = 8000;
pub const RELAX_STEP_TIMEOUT_MS: u64 = 4000;
const QUERY_CANCELED: &str = "57014";

pub const DAY: &str = "COALESCE($1::date, current_date)";
const LAW_TYPE: &str = "($2::text[] IS NULL OR l.law_type = ANY($2::text[]))";
const TEXT_LAW_IDS: &str = "($4::text[] IS NULL OR l.law_id = ANY($4::text[]))";
const TEXT_FIRST_PATTERN: usize = 5;
const VECTOR_LAW_IDS: &str = "($6::text[] IS NULL OR l.law_id = ANY($6::text[]))";
pub const OPEN_ARTICLE: &str = "upper_inf(av.sys_period)";
const OPEN_ANNEX: &str = "upper_inf(a.sys_period)";
const NODE_TYPES: &str = "('row','row_item','rule_para','note','section')";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnitRef {
    Article(i64),
    Annex(i64),
    Node(String),
}

pub const VALID_AV: &str = "(NOT isempty(av.valid_period) \
     AND (lower_inf(av.valid_period) OR lower(av.valid_period) <= COALESCE($1::date, current_date)) \
     AND (upper_inf(av.valid_period) OR upper(av.valid_period) > COALESCE($1::date, current_date)))";
pub const VALID_A: &str = "(NOT isempty(a.valid_period) \
     AND (lower_inf(a.valid_period) OR lower(a.valid_period) <= COALESCE($1::date, current_date)) \
     AND (upper_inf(a.valid_period) OR upper(a.valid_period) > COALESCE($1::date, current_date)))";

fn text_sql(terms: &Terms, m: usize, weights: &LawWeights) -> Option<String> {
    let w = weights.sql("l.kind", "l.law_type");
    let wx = weights.annex_sql("l.kind", "l.law_type");
    Some(format!(
        "(SELECT 'a' AS src, av.row_id AS id, {sa} * {w} AS s, length(av.body) AS len FROM article_version av \
           JOIN law l ON l.law_id = av.law_id \
           WHERE {ma} AND {OPEN_ARTICLE} AND {VALID_AV} AND {LAW_TYPE} AND {TEXT_LAW_IDS} \
           ORDER BY s DESC, len, av.row_id LIMIT $3) \
         UNION ALL \
         (SELECT 'x' AS src, a.row_id AS id, {sx} * {wx} AS s, length(a.raw_text) AS len FROM annex a \
           JOIN law l ON l.law_id = a.law_id \
           WHERE {mx} AND {OPEN_ANNEX} AND {VALID_A} AND {LAW_TYPE} AND {TEXT_LAW_IDS} \
           ORDER BY s DESC, len, a.row_id LIMIT $3) \
         ORDER BY s DESC, len, src, id LIMIT $3",
        sa = terms.contained("av.body", TEXT_FIRST_PATTERN),
        ma = terms.at_least("av.body", TEXT_FIRST_PATTERN, m)?,
        sx = terms.contained("a.raw_text", TEXT_FIRST_PATTERN),
        mx = terms.at_least("a.raw_text", TEXT_FIRST_PATTERN, m)?,
    ))
}

pub async fn text_candidates(
    pool: &PgPool,
    q: &LawQuery,
    terms: &Terms,
    weights: &LawWeights,
    law_ids: Option<&[String]>,
) -> Result<Vec<UnitRef>, RetrievalError> {
    let patterns = terms.like_patterns();
    let mut rows = Vec::new();
    for (step, m) in terms.relaxations().into_iter().enumerate() {
        let Some(sql) = text_sql(terms, m, weights) else { break };
        let mut query = sqlx::query(AssertSqlSafe(sql))
            .bind(q.as_of)
            .bind(q.law_type.clone())
            .bind(CANDIDATES)
            .bind(law_ids);
        for p in &patterns {
            query = query.bind(p.clone());
        }
        let limit = if step == 0 { FIRST_STEP_TIMEOUT_MS } else { RELAX_STEP_TIMEOUT_MS };
        let mut tx = pool.begin().await?;
        sqlx::query(AssertSqlSafe(format!("SET LOCAL statement_timeout = {limit}")))
            .execute(&mut *tx)
            .await?;
        match query.fetch_all(&mut *tx).await {
            Ok(found) => rows = found,
            Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some(QUERY_CANCELED) => {
                tracing::info!(step, kept = rows.len(), "text candidate step cut by its time limit");
                break;
            }
            Err(e) => return Err(e.into()),
        }
        if rows.len() as i64 >= CANDIDATES {
            break;
        }
    }
    let mut out = Vec::with_capacity(rows.len());
    let mut annexes = Vec::new();
    for row in rows {
        let src: String = row.try_get(0)?;
        let id: i64 = row.try_get(1)?;
        if src == "a" {
            out.push(UnitRef::Article(id));
        } else {
            annexes.push(id);
            out.push(UnitRef::Annex(id));
        }
    }
    if annexes.is_empty() {
        return Ok(out);
    }
    let best = best_nodes(pool, &annexes, terms).await?;
    Ok(out
        .into_iter()
        .map(|r| match r {
            UnitRef::Annex(id) => best.get(&id).cloned().map(UnitRef::Node).unwrap_or(UnitRef::Annex(id)),
            other => other,
        })
        .collect())
}

async fn best_nodes(
    pool: &PgPool,
    annexes: &[i64],
    terms: &Terms,
) -> Result<HashMap<i64, String>, RetrievalError> {
    let score = terms.contained("(n.text || ' ' || COALESCE(n.cells::text, ''))", 2);
    let sql = format!(
        "SELECT DISTINCT ON (n.annex_row_id) n.annex_row_id, n.node_id, {score} AS s FROM annex_node n \
         WHERE n.annex_row_id = ANY($1) AND n.node_type IN {NODE_TYPES} \
         ORDER BY n.annex_row_id, s DESC, length(n.text), n.node_id"
    );
    let mut query = sqlx::query(AssertSqlSafe(sql)).bind(annexes);
    for p in terms.like_patterns() {
        query = query.bind(p);
    }
    let mut out = HashMap::new();
    for row in query.fetch_all(pool).await? {
        let s: i32 = row.try_get(2)?;
        if s > 0 {
            out.insert(row.try_get(0)?, row.try_get(1)?);
        }
    }
    Ok(out)
}

pub async fn vector_ready(pool: &PgPool, registry: &str) -> Result<bool, RetrievalError> {
    let ready: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM law_text_embedding e JOIN embedding_model m ON m.model_id = e.model_id \
           WHERE m.name = $1) \
         OR EXISTS (SELECT 1 FROM annex_node n JOIN embedding_model m ON m.model_id = n.embed_model_id \
           WHERE m.name = $1 AND n.embedding IS NOT NULL)",
    )
    .bind(registry)
    .fetch_one(pool)
    .await?;
    Ok(ready)
}

pub async fn vector_candidates(
    pool: &PgPool,
    q: &LawQuery,
    registry: &str,
    vector: &str,
    law_ids: Option<&[String]>,
) -> Result<Vec<UnitRef>, RetrievalError> {
    let sql = format!(
        "(SELECT 'a' AS src, av.row_id::text AS id, e.embedding <=> CAST($5::text AS halfvec) AS d \
           FROM law_text_embedding e JOIN embedding_model m ON m.model_id = e.model_id \
           JOIN article_version av ON av.content_hash = e.content_hash \
           JOIN law l ON l.law_id = av.law_id \
           WHERE m.name = $4 AND {OPEN_ARTICLE} AND {VALID_AV} AND {LAW_TYPE} AND {VECTOR_LAW_IDS} \
           ORDER BY d LIMIT $3) \
         UNION ALL \
         (SELECT 'n' AS src, n.node_id AS id, n.embedding <=> CAST($5::text AS halfvec) AS d \
           FROM annex_node n JOIN embedding_model nm ON nm.model_id = n.embed_model_id \
           JOIN annex a ON a.row_id = n.annex_row_id \
           JOIN law l ON l.law_id = a.law_id \
           WHERE nm.name = $4 AND n.embedding IS NOT NULL AND {OPEN_ANNEX} AND {VALID_A} AND {LAW_TYPE} AND {VECTOR_LAW_IDS} \
           ORDER BY d LIMIT $3) \
         ORDER BY d LIMIT $3"
    );
    let rows = sqlx::query(AssertSqlSafe(sql))
        .bind(q.as_of)
        .bind(q.law_type.clone())
        .bind(CANDIDATES)
        .bind(registry)
        .bind(vector)
        .bind(law_ids)
        .fetch_all(pool)
        .await?;
    let mut out: Vec<UnitRef> = Vec::with_capacity(rows.len());
    for row in rows {
        let src: String = row.try_get(0)?;
        let id: String = row.try_get(1)?;
        let unit = if src == "a" {
            UnitRef::Article(id.parse().map_err(|_| RetrievalError::Argument("vector id"))?)
        } else {
            UnitRef::Node(id)
        };
        if !out.contains(&unit) {
            out.push(unit);
        }
    }
    Ok(out)
}

pub async fn resolve_law(pool: &PgPool, law: &str) -> Result<Option<(String, String, String)>, RetrievalError> {
    let law = law.trim();
    if law.is_empty() {
        return Ok(None);
    }
    let row = sqlx::query_as::<_, (String, String, String)>(
        "SELECT law_id, name, kind FROM law \
         WHERE law_id = $1 OR regexp_replace(name, '\\s', '', 'g') = $2 \
         ORDER BY (law_id = $1) DESC, (kind = 'law') DESC, law_id LIMIT 1",
    )
    .bind(law)
    .bind(compact(law))
    .fetch_optional(pool)
    .await?;
    if row.is_some() {
        return Ok(row);
    }
    let Some(alias) = lawnames::lookup(law) else { return Ok(None) };
    Ok(sqlx::query_as::<_, (String, String, String)>("SELECT law_id, name, kind FROM law WHERE law_id = $1")
        .bind(&alias.law_id)
        .fetch_optional(pool)
        .await?)
}

pub async fn direct_candidates(
    pool: &PgPool,
    direct: &Direct,
    as_of: Option<NaiveDate>,
) -> Result<Vec<UnitRef>, RetrievalError> {
    let Some((law_id, _, _)) = resolve_law(pool, &direct.law).await? else {
        return Ok(Vec::new());
    };
    match &direct.target {
        Target::Article(no) => {
            let ids: Vec<i64> = sqlx::query_scalar(AssertSqlSafe(format!(
                "SELECT av.row_id FROM article_version av \
                 WHERE av.law_id = $2 AND av.article_no = $3 AND {OPEN_ARTICLE} AND {VALID_AV} \
                 ORDER BY av.row_id"
            )))
            .bind(as_of)
            .bind(&law_id)
            .bind(no)
            .fetch_all(pool)
            .await?;
            Ok(ids.into_iter().map(UnitRef::Article).collect())
        }
        Target::Annex(name) => {
            let ids: Vec<i64> = sqlx::query_scalar(AssertSqlSafe(format!(
                "SELECT a.row_id FROM annex a \
                 WHERE a.annex_key = $2 AND {OPEN_ANNEX} AND {VALID_A} ORDER BY a.row_id"
            )))
            .bind(as_of)
            .bind(name.key(&law_id))
            .fetch_all(pool)
            .await?;
            Ok(ids.into_iter().map(UnitRef::Annex).collect())
        }
    }
}

const ARTICLE_DETAILS: &str = "SELECT av.row_id, l.name, av.article_no, COALESCE(av.title, ''), av.valid_from, \
     l.kind, av.source_lsi_seq, av.body, l.law_id, av.article_version_id, \
     (SELECT min(nx.valid_from) FROM article_version nx WHERE nx.article_key = av.article_key \
        AND upper(nx.sys_period) IS NULL AND nx.valid_from > COALESCE($2::date, current_date)) \
     FROM article_version av JOIN law l ON l.law_id = av.law_id WHERE av.row_id = ANY($1)";

const ANNEX_DETAILS: &str = "SELECT a.row_id, l.name, a.annex_type, a.annex_no, a.annex_sub_no, a.title, a.valid_from, \
     l.kind, a.lsi_seq, COALESCE(a.raw_text, ''), l.law_id, a.annex_version_id \
     FROM annex a JOIN law l ON l.law_id = a.law_id WHERE a.row_id = ANY($1)";

const NODE_DETAILS: &str = "SELECT n.node_id, l.name, a.annex_type, a.annex_no, a.annex_sub_no, a.title, a.valid_from, \
     l.kind, a.lsi_seq, n.chunk, l.law_id, a.annex_version_id, n.node_type \
     FROM annex_node n JOIN annex a ON a.row_id = n.annex_row_id JOIN law l ON l.law_id = a.law_id \
     WHERE n.node_id = ANY($1)";

const SNIPPET_CHARS: usize = 600;

fn head(text: &str) -> String {
    text.chars().take(SNIPPET_CHARS).collect()
}

pub async fn hits(
    pool: &PgPool,
    ranked: &[(UnitRef, f64, String)],
    as_of: Option<NaiveDate>,
    units: &HashMap<i64, Vec<String>>,
) -> Result<Vec<LawHit>, RetrievalError> {
    let mut articles = Vec::new();
    let mut annexes = Vec::new();
    let mut nodes = Vec::new();
    for (r, _, _) in ranked {
        match r {
            UnitRef::Article(id) => articles.push(*id),
            UnitRef::Annex(id) => annexes.push(*id),
            UnitRef::Node(id) => nodes.push(id.clone()),
        }
    }
    let mut found: HashMap<UnitRef, LawHit> = HashMap::new();
    if !articles.is_empty() {
        for row in sqlx::query(ARTICLE_DETAILS).bind(&articles).bind(as_of).fetch_all(pool).await? {
            let row_id: i64 = row.try_get(0)?;
            found.insert(
                UnitRef::Article(row_id),
                LawHit {
                    law_name: row.try_get(1)?,
                    unit_key: row.try_get(2)?,
                    title: row.try_get(3)?,
                    enforcement_date: row.try_get(4)?,
                    kind: row.try_get(5)?,
                    lsi_seq: row.try_get(6)?,
                    text: row.try_get(7)?,
                    score: 0.0,
                    law_id: row.try_get(8)?,
                    unit_kind: "article".into(),
                    version_id: row.try_get(9)?,
                    node_id: None,
                    scheduled_from: row.try_get(10)?,
                    matched: String::new(),
                    matched_units: units.get(&row_id).cloned().unwrap_or_default(),
                },
            );
        }
    }
    if !annexes.is_empty() {
        for row in sqlx::query(ANNEX_DETAILS).bind(&annexes).fetch_all(pool).await? {
            let raw: String = row.try_get(9)?;
            found.insert(
                UnitRef::Annex(row.try_get(0)?),
                LawHit {
                    law_name: row.try_get(1)?,
                    unit_key: annex_label(&row.try_get::<String, _>(2)?, row.try_get(3)?, row.try_get(4)?),
                    title: row.try_get(5)?,
                    enforcement_date: row.try_get(6)?,
                    kind: row.try_get(7)?,
                    lsi_seq: row.try_get(8)?,
                    text: head(&raw),
                    score: 0.0,
                    law_id: row.try_get(10)?,
                    unit_kind: "annex".into(),
                    version_id: row.try_get(11)?,
                    node_id: None,
                    scheduled_from: None,
                    matched: String::new(),
                    matched_units: Vec::new(),
                },
            );
        }
    }
    if !nodes.is_empty() {
        for row in sqlx::query(NODE_DETAILS).bind(&nodes).fetch_all(pool).await? {
            let id: String = row.try_get(0)?;
            let node_type: String = row.try_get(12)?;
            found.insert(
                UnitRef::Node(id.clone()),
                LawHit {
                    law_name: row.try_get(1)?,
                    unit_key: annex_label(&row.try_get::<String, _>(2)?, row.try_get(3)?, row.try_get(4)?),
                    title: row.try_get(5)?,
                    enforcement_date: row.try_get(6)?,
                    kind: row.try_get(7)?,
                    lsi_seq: row.try_get(8)?,
                    text: row.try_get(9)?,
                    score: 0.0,
                    law_id: row.try_get(10)?,
                    unit_kind: format!("annex_{node_type}"),
                    version_id: row.try_get(11)?,
                    node_id: Some(id),
                    scheduled_from: None,
                    matched: String::new(),
                    matched_units: Vec::new(),
                },
            );
        }
    }
    Ok(ranked
        .iter()
        .filter_map(|(r, score, matched)| {
            found.remove(r).map(|mut hit| {
                hit.score = *score;
                hit.matched = matched.clone();
                hit
            })
        })
        .collect())
}

pub async fn unit_paths(pool: &PgPool, ids: &[i64]) -> Result<HashMap<i64, String>, RetrievalError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(i64, String)> = sqlx::query_as("SELECT unit_id, path FROM article_unit WHERE unit_id = ANY($1)")
        .bind(ids)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().collect())
}

fn deleted(article_no: &str, body: &str) -> bool {
    let body = compact(body);
    body.strip_prefix(&compact(article_no))
        .map(|rest| rest.starts_with("삭제"))
        .unwrap_or(false)
}

fn article_sql() -> String {
    format!(
        "SELECT l.name, av.article_no, COALESCE(av.title, ''), av.body, av.valid_from, l.kind, av.source_lsi_seq, \
           av.effective_status, av.article_version_id, lower(av.valid_period), upper(av.valid_period), av.end_reason, \
           (SELECT min(nx.valid_from) FROM article_version nx WHERE nx.article_key = av.article_key \
              AND upper(nx.sys_period) IS NULL AND nx.valid_from > {DAY}) \
         FROM article_version av JOIN law l ON l.law_id = av.law_id \
         WHERE av.law_id = $2 AND regexp_replace(av.article_no, '\\s', '', 'g') = $3 \
           AND {OPEN_ARTICLE} AND {VALID_AV} \
         ORDER BY av.row_id LIMIT 1"
    )
}

fn annex_sql() -> String {
    format!(
        "SELECT l.name, a.title, COALESCE(a.raw_text, ''), a.valid_from, l.kind, a.lsi_seq, a.annex_version_id, \
           lower(a.valid_period), upper(a.valid_period), a.end_reason, \
           a.source_pdf_url, a.source_hwp_url, a.source_image_url, \
           (SELECT min(nx.valid_from) FROM annex nx WHERE nx.annex_key = a.annex_key \
              AND upper(nx.sys_period) IS NULL AND nx.valid_from > {DAY}) \
         FROM annex a JOIN law l ON l.law_id = a.law_id \
         WHERE a.annex_key = $2 AND {OPEN_ANNEX} AND {VALID_A} \
         ORDER BY a.row_id LIMIT 1"
    )
}

pub async fn read_law_unit(
    pool: &PgPool,
    law: &str,
    unit: &str,
    as_of: Option<NaiveDate>,
) -> Result<LawUnit, RetrievalError> {
    let unit = compact(unit);
    if law.trim().is_empty() || unit.is_empty() {
        return Err(RetrievalError::Argument("law and unit"));
    }
    let (law_id, _, _) = resolve_law(pool, law)
        .await?
        .ok_or(RetrievalError::NotFound("law unit"))?;
    if let Some(name) = AnnexName::parse(&unit) {
        return read_annex_unit(pool, &law_id, &name, as_of).await;
    }
    let row = sqlx::query(AssertSqlSafe(article_sql()))
        .bind(as_of)
        .bind(&law_id)
        .bind(&unit)
        .fetch_optional(pool)
        .await?
        .ok_or(RetrievalError::NotFound("law unit"))?;
    let article_no: String = row.try_get(1)?;
    let body: String = row.try_get(3)?;
    Ok(LawUnit {
        law_name: row.try_get(0)?,
        deleted: deleted(&article_no, &body),
        unit_key: article_no,
        title: row.try_get(2)?,
        body,
        content_status: "text".into(),
        source_urls: Value::Array(Vec::new()),
        enforcement_date: row.try_get(4)?,
        version: LawVersion {
            kind: row.try_get(5)?,
            lsi_seq: row.try_get(6)?,
            status: row.try_get(7)?,
            version_id: row.try_get(8)?,
            valid_from: row.try_get(9)?,
            valid_to: row.try_get(10)?,
            end_reason: row.try_get(11)?,
        },
        law_id,
        unit_kind: "article".into(),
        scheduled_from: row.try_get(12)?,
    })
}

async fn read_annex_unit(
    pool: &PgPool,
    law_id: &str,
    name: &AnnexName,
    as_of: Option<NaiveDate>,
) -> Result<LawUnit, RetrievalError> {
    let row = sqlx::query(AssertSqlSafe(annex_sql()))
        .bind(as_of)
        .bind(name.key(law_id))
        .fetch_optional(pool)
        .await?
        .ok_or(RetrievalError::NotFound("law unit"))?;
    let body: String = row.try_get(2)?;
    let urls: Vec<Value> = [10, 11, 12]
        .iter()
        .filter_map(|i| row.try_get::<Option<String>, _>(*i).ok().flatten())
        .filter(|u| !u.is_empty())
        .map(Value::String)
        .collect();
    let valid_from: Option<NaiveDate> = row.try_get(3)?;
    Ok(LawUnit {
        law_name: row.try_get(0)?,
        unit_key: name.label(),
        title: row.try_get(1)?,
        content_status: if body.trim().is_empty() { "link_only".into() } else { "text".into() },
        body,
        deleted: false,
        source_urls: Value::Array(urls),
        enforcement_date: valid_from,
        version: LawVersion {
            kind: row.try_get(4)?,
            lsi_seq: row.try_get(5)?,
            status: "effective".into(),
            version_id: row.try_get(6)?,
            valid_from: row.try_get(7)?,
            valid_to: row.try_get(8)?,
            end_reason: row.try_get(9)?,
        },
        law_id: law_id.to_string(),
        unit_kind: "annex".into(),
        scheduled_from: row.try_get(13)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleted_reads_the_word_right_after_the_article_number() {
        assert!(deleted("제4조", "제4조 삭제 <2015. 1. 1.>"));
        assert!(!deleted("제4조", "제4조(정의) 이 법에서 삭제란 ..."));
        assert!(!deleted("제4조", "① 국가는 ..."));
    }
}
