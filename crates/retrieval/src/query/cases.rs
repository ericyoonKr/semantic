use chrono::NaiveDate;
use sqlx::{AssertSqlSafe, PgPool, Row};

use crate::error::RetrievalError;
use crate::model::{CaseDoc, CaseHit, CaseQuery, CaseSection};
use crate::query::fusion::CANDIDATES;
use crate::query::terms::Terms;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextCandidate {
    pub section_id: i64,
    pub position: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VectorCandidate {
    pub section_id: i64,
    pub chunk_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick {
    pub section_id: i64,
    pub chunk_id: i64,
    pub position: i32,
    pub score: f64,
}

const FILTERS: &str = "NOT d.is_bundle \
     AND ($1::text[] IS NULL OR d.field = ANY($1::text[])) \
     AND ($2::text[] IS NULL OR c.level = ANY($2::text[])) \
     AND ($3::date IS NULL OR d.decided_on >= $3::date) \
     AND ($4::date IS NULL OR d.decided_on <= $4::date)";

fn bind_filters<'q, O>(
    query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments>,
    q: &CaseQuery,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments> {
    query
        .bind(q.field.clone())
        .bind(q.court_level.clone())
        .bind(q.decided_from)
        .bind(q.decided_to)
        .bind(CANDIDATES)
}

pub async fn text_candidates(
    pool: &PgPool,
    q: &CaseQuery,
    terms: &Terms,
) -> Result<Vec<TextCandidate>, RetrievalError> {
    let sql = format!(
        "SELECT s.section_id, {position}::int4 FROM decision_section s \
         JOIN decision d ON d.decision_id = s.decision_id \
         JOIN court c ON c.court_id = d.court_id \
         WHERE {FILTERS} AND {matches} \
         ORDER BY {score} DESC, s.section_id LIMIT $5",
        position = terms.first_position("s.body", 6),
        matches = terms.matches("s.body", 6),
        score = terms.score("s.body", 6),
    );
    let mut query = bind_filters(sqlx::query_as::<_, (i64, i32)>(AssertSqlSafe(sql)), q);
    for word in terms.words() {
        query = query.bind(word.clone());
    }
    let rows = query.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(section_id, position)| TextCandidate {
            section_id,
            position,
        })
        .collect())
}

const VECTOR: &str = "SELECT ch.section_id, ch.chunk_id FROM chunk_embedding e \
     JOIN embedding_model m ON m.model_id = e.model_id \
     JOIN decision_chunk ch ON ch.chunk_id = e.chunk_id \
     JOIN decision_section s ON s.section_id = ch.section_id \
     JOIN decision d ON d.decision_id = s.decision_id \
     JOIN court c ON c.court_id = d.court_id \
     WHERE m.name = $6 AND ";

pub async fn vector_candidates(
    pool: &PgPool,
    q: &CaseQuery,
    registry: &str,
    vector: &str,
) -> Result<Vec<VectorCandidate>, RetrievalError> {
    let sql = format!(
        "{VECTOR}{FILTERS} ORDER BY e.embedding <=> CAST($7::text AS halfvec) LIMIT $5"
    );
    let rows = bind_filters(sqlx::query_as::<_, (i64, i64)>(AssertSqlSafe(sql)), q)
        .bind(registry)
        .bind(vector)
        .fetch_all(pool)
        .await?;
    let mut out: Vec<VectorCandidate> = Vec::with_capacity(rows.len());
    for (section_id, chunk_id) in rows {
        if !out.iter().any(|c| c.section_id == section_id) {
            out.push(VectorCandidate {
                section_id,
                chunk_id,
            });
        }
    }
    Ok(out)
}

const DETAILS: &str = "SELECT d.decision_id, c.name, d.case_no_display, d.decided_on, d.field, s.role, \
       CASE WHEN k.chunk_id IS NULL THEN s.body \
            ELSE substring(s.body FROM k.char_start + 1 FOR k.char_end - k.char_start) END, p.ord \
     FROM unnest($1::int8[], $2::int8[], $3::int4[]) WITH ORDINALITY AS p(section_id, chunk_id, pos, ord) \
     JOIN decision_section s ON s.section_id = p.section_id \
     JOIN decision d ON d.decision_id = s.decision_id \
     JOIN court c ON c.court_id = d.court_id \
     LEFT JOIN LATERAL ( \
       SELECT ch.chunk_id, ch.char_start, ch.char_end FROM decision_chunk ch \
       WHERE ch.section_id = s.section_id \
       ORDER BY (ch.chunk_id = p.chunk_id) DESC, \
                (p.pos > 0 AND ch.char_start < p.pos AND ch.char_end >= p.pos) DESC, ch.ord \
       LIMIT 1 \
     ) k ON true \
     ORDER BY p.ord";

pub async fn hits(pool: &PgPool, picks: &[Pick]) -> Result<Vec<CaseHit>, RetrievalError> {
    let sections: Vec<i64> = picks.iter().map(|p| p.section_id).collect();
    let chunks: Vec<i64> = picks.iter().map(|p| p.chunk_id).collect();
    let positions: Vec<i32> = picks.iter().map(|p| p.position).collect();
    let rows = sqlx::query(DETAILS)
        .bind(&sections)
        .bind(&chunks)
        .bind(&positions)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let ord: i64 = row.try_get(7)?;
        let score = usize::try_from(ord - 1)
            .ok()
            .and_then(|i| picks.get(i))
            .map(|p| p.score)
            .unwrap_or(0.0);
        out.push(CaseHit {
            decision_id: row.try_get(0)?,
            court: row.try_get(1)?,
            case_no: row.try_get(2)?,
            decided_on: row.try_get(3)?,
            field: row.try_get(4)?,
            section_role: row.try_get(5)?,
            text: row.try_get(6)?,
            score,
        });
    }
    Ok(out)
}

const HEAD: &str = "SELECT c.name, d.case_no_display, d.decided_on, d.field \
     FROM decision d JOIN court c ON c.court_id = d.court_id WHERE d.decision_id = $1";

const SECTIONS: &str = "SELECT s.role, COALESCE(s.heading, ''), s.body FROM decision_section s \
     LEFT JOIN section_role r ON r.role = s.role \
     WHERE s.decision_id = $1 AND ($2::text[] IS NULL OR s.role = ANY($2::text[]) OR r.label = ANY($2::text[])) \
     ORDER BY s.ord";

pub async fn read_case(
    pool: &PgPool,
    decision_id: i64,
    sections: Option<&[String]>,
) -> Result<CaseDoc, RetrievalError> {
    let head: Option<(String, String, NaiveDate, String)> = sqlx::query_as(HEAD)
        .bind(decision_id)
        .fetch_optional(pool)
        .await?;
    let (court, case_no, decided_on, field) = head.ok_or(RetrievalError::NotFound("decision"))?;
    let wanted: Option<Vec<String>> = sections.map(<[String]>::to_vec);
    let rows: Vec<(String, String, String)> = sqlx::query_as(SECTIONS)
        .bind(decision_id)
        .bind(wanted)
        .fetch_all(pool)
        .await?;
    Ok(CaseDoc {
        court,
        case_no,
        decided_on,
        field,
        sections: rows
            .into_iter()
            .map(|(role, heading, body)| CaseSection {
                role,
                heading,
                body,
            })
            .collect(),
    })
}
