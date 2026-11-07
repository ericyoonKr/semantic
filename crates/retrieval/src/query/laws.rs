use std::collections::HashMap;

use chrono::NaiveDate;
use sqlx::{AssertSqlSafe, PgPool, Row};

use crate::error::RetrievalError;
use crate::model::{LawHit, LawQuery, LawUnit, LawVersion};
use crate::query::fusion::CANDIDATES;
use crate::query::terms::{Terms, compact};

pub fn version_at(slot: usize) -> String {
    format!(
        "((${slot}::date IS NULL AND v.status = 'current') OR (${slot}::date IS NOT NULL AND v.enforcement_date <= ${slot}::date))"
    )
}

fn versions() -> String {
    format!(
        "SELECT DISTINCT ON (v.law_id) v.law_id, v.kind, v.lsi_seq FROM law_version v WHERE {} \
         ORDER BY v.law_id, v.enforcement_date DESC NULLS LAST, v.lsi_seq DESC",
        version_at(1)
    )
}

const LAW_TYPE: &str = "($2::text[] IS NULL OR l.law_type = ANY($2::text[]))";

pub async fn text_candidates(
    pool: &PgPool,
    q: &LawQuery,
    terms: &Terms,
) -> Result<Vec<i64>, RetrievalError> {
    let sql = format!(
        "WITH ver AS ({ver}) SELECT u.unit_id FROM ver \
         JOIN law l ON l.law_id = ver.law_id \
         JOIN law_unit u ON u.kind = ver.kind AND u.lsi_seq = ver.lsi_seq \
         WHERE NOT u.deleted AND {LAW_TYPE} AND {matches} \
         ORDER BY {score} DESC, u.unit_id LIMIT $3",
        ver = versions(),
        matches = terms.matches("u.body", 4),
        score = terms.score("u.body", 4),
    );
    let mut query = sqlx::query_scalar::<_, i64>(AssertSqlSafe(sql))
        .bind(q.as_of)
        .bind(q.law_type.clone())
        .bind(CANDIDATES);
    for word in terms.words() {
        query = query.bind(word.clone());
    }
    Ok(query.fetch_all(pool).await?)
}

pub async fn vector_candidates(
    pool: &PgPool,
    q: &LawQuery,
    registry: &str,
    vector: &str,
) -> Result<Vec<i64>, RetrievalError> {
    let sql = format!(
        "WITH ver AS ({ver}) SELECT u.unit_id FROM law_unit_embedding e \
         JOIN embedding_model m ON m.model_id = e.model_id \
         JOIN law_unit u ON u.unit_id = e.unit_id \
         JOIN ver ON ver.kind = u.kind AND ver.lsi_seq = u.lsi_seq \
         JOIN law l ON l.law_id = ver.law_id \
         WHERE m.name = $4 AND NOT u.deleted AND {LAW_TYPE} \
         ORDER BY e.embedding <=> CAST($5::text AS halfvec) LIMIT $3",
        ver = versions(),
    );
    let ids = sqlx::query_scalar::<_, i64>(AssertSqlSafe(sql))
        .bind(q.as_of)
        .bind(q.law_type.clone())
        .bind(CANDIDATES)
        .bind(registry)
        .bind(vector)
        .fetch_all(pool)
        .await?;
    let mut out: Vec<i64> = Vec::with_capacity(ids.len());
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

const DETAILS: &str = "SELECT u.unit_id, l.name, u.unit_key, COALESCE(u.title, ''), \
     COALESCE(u.enforcement_date, v.enforcement_date), u.kind, u.lsi_seq, u.body \
     FROM law_unit u JOIN law_version v ON v.kind = u.kind AND v.lsi_seq = u.lsi_seq \
     JOIN law l ON l.law_id = v.law_id WHERE u.unit_id = ANY($1)";

pub async fn hits(pool: &PgPool, ranked: &[(i64, f64)]) -> Result<Vec<LawHit>, RetrievalError> {
    let ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();
    let rows = sqlx::query(DETAILS).bind(&ids).fetch_all(pool).await?;
    let mut found: HashMap<i64, LawHit> = HashMap::with_capacity(rows.len());
    for row in rows {
        let id: i64 = row.try_get(0)?;
        found.insert(
            id,
            LawHit {
                law_name: row.try_get(1)?,
                unit_key: row.try_get(2)?,
                title: row.try_get(3)?,
                enforcement_date: row.try_get(4)?,
                kind: row.try_get(5)?,
                lsi_seq: row.try_get(6)?,
                text: row.try_get(7)?,
                score: 0.0,
            },
        );
    }
    Ok(ranked
        .iter()
        .filter_map(|(id, score)| {
            found.remove(id).map(|mut hit| {
                hit.score = *score;
                hit
            })
        })
        .collect())
}

fn read_sql() -> String {
    format!(
        "WITH target AS ( \
           SELECT law_id, name FROM law \
           WHERE law_id = $1 OR regexp_replace(name, '\\s', '', 'g') = $2 \
           ORDER BY (law_id = $1) DESC, (kind = 'law') DESC, law_id LIMIT 1 \
         ), ver AS ( \
           SELECT v.kind, v.lsi_seq, v.enforcement_date, v.status \
           FROM law_version v JOIN target t ON t.law_id = v.law_id \
           WHERE {} \
           ORDER BY v.enforcement_date DESC NULLS LAST, v.lsi_seq DESC LIMIT 1 \
         ) \
         SELECT t.name, u.unit_key, COALESCE(u.title, ''), u.body, u.deleted, u.content_status, \
           u.source_urls, COALESCE(u.enforcement_date, ver.enforcement_date), ver.kind, ver.lsi_seq, ver.status \
         FROM target t CROSS JOIN ver \
         JOIN law_unit u ON u.kind = ver.kind AND u.lsi_seq = ver.lsi_seq \
         WHERE regexp_replace(u.unit_key, '\\s', '', 'g') = $4 \
         ORDER BY u.unit_id LIMIT 1",
        version_at(3)
    )
}

pub async fn read_law_unit(
    pool: &PgPool,
    law: &str,
    unit: &str,
    as_of: Option<NaiveDate>,
) -> Result<LawUnit, RetrievalError> {
    let law = law.trim();
    let unit = compact(unit);
    if law.is_empty() || unit.is_empty() {
        return Err(RetrievalError::Argument("law and unit"));
    }
    let row = sqlx::query(AssertSqlSafe(read_sql()))
        .bind(law)
        .bind(compact(law))
        .bind(as_of)
        .bind(unit)
        .fetch_optional(pool)
        .await?
        .ok_or(RetrievalError::NotFound("law unit"))?;
    Ok(LawUnit {
        law_name: row.try_get(0)?,
        unit_key: row.try_get(1)?,
        title: row.try_get(2)?,
        body: row.try_get(3)?,
        deleted: row.try_get(4)?,
        content_status: row.try_get(5)?,
        source_urls: row.try_get(6)?,
        enforcement_date: row.try_get(7)?,
        version: LawVersion {
            kind: row.try_get(8)?,
            lsi_seq: row.try_get(9)?,
            status: row.try_get(10)?,
        },
    })
}
