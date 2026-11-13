use sqlx::{AssertSqlSafe, PgPool};

use super::{bind_filters, filters};
use crate::error::RetrievalError;
use crate::model::CaseQuery;
use crate::query::terms::Terms;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub section_id: i64,
    pub decision_id: i64,
    pub chunk_id: i64,
    pub position: i32,
}

impl Candidate {
    pub fn section(section_id: i64, decision_id: i64) -> Self {
        Self { section_id, decision_id, chunk_id: 0, position: 0 }
    }
}

const TEXT_FIRST_SLOT: usize = 7;

fn rare_filter(column: &str, rare: &[usize]) -> String {
    let parts: Vec<String> = rare.iter().map(|i| format!("{column} LIKE ${}", TEXT_FIRST_SLOT + i)).collect();
    format!("({})", parts.join(" OR "))
}

const SECTION_FROM: &str = "FROM decision_section s \
     JOIN decision d ON d.decision_id = s.decision_id \
     JOIN court c ON c.court_id = d.court_id";

pub async fn text(pool: &PgPool, q: &CaseQuery, terms: &Terms, rare: &[usize]) -> Result<Vec<Candidate>, RetrievalError> {
    let words = TEXT_FIRST_SLOT + terms.words().len();
    let sql = format!(
        "SELECT t.section_id, t.decision_id, {position}::int4 FROM ( \
           SELECT s.section_id, d.decision_id, {score} AS score, length(s.body) AS len {SECTION_FROM} \
           WHERE {filters} AND {any} \
           ORDER BY score DESC, len, s.section_id LIMIT $5) t \
         JOIN decision_section b ON b.section_id = t.section_id \
         ORDER BY t.score DESC, t.len, t.section_id",
        position = terms.first_position("b.body", words),
        filters = filters(),
        score = terms.contained("s.body", TEXT_FIRST_SLOT),
        any = if rare.is_empty() {
            terms
                .at_least("s.body", TEXT_FIRST_SLOT, 1)
                .unwrap_or_else(|| format!("{} > 0", terms.contained("s.body", TEXT_FIRST_SLOT)))
        } else {
            rare_filter("s.body", rare)
        },
    );
    let mut query = bind_filters(sqlx::query_as::<_, (i64, i64, i32)>(AssertSqlSafe(sql)), q);
    for pattern in terms.like_patterns() {
        query = query.bind(pattern);
    }
    for word in terms.words() {
        query = query.bind(word.clone());
    }
    let rows = query.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(section_id, decision_id, position)| Candidate { position, ..Candidate::section(section_id, decision_id) })
        .collect())
}

const VECTOR_READY: &str = "SELECT EXISTS (SELECT 1 FROM chunk_embedding e \
     JOIN embedding_model m ON m.model_id = e.model_id WHERE m.name = $1)";

pub async fn vector_ready(pool: &PgPool, registry: &str) -> Result<bool, RetrievalError> {
    Ok(sqlx::query_scalar(VECTOR_READY).bind(registry).fetch_one(pool).await?)
}

pub async fn vector(pool: &PgPool, q: &CaseQuery, registry: &str, literal: &str) -> Result<Vec<Candidate>, RetrievalError> {
    let sql = format!(
        "SELECT ch.section_id, d.decision_id, ch.chunk_id FROM chunk_embedding e \
         JOIN embedding_model m ON m.model_id = e.model_id \
         JOIN decision_chunk ch ON ch.chunk_id = e.chunk_id \
         JOIN decision_section s ON s.section_id = ch.section_id \
         JOIN decision d ON d.decision_id = s.decision_id \
         JOIN court c ON c.court_id = d.court_id \
         WHERE m.name = $7 AND {filters} \
         ORDER BY e.embedding <=> CAST($8::text AS halfvec) LIMIT $5",
        filters = filters(),
    );
    let rows = bind_filters(sqlx::query_as::<_, (i64, i64, i64)>(AssertSqlSafe(sql)), q)
        .bind(registry)
        .bind(literal)
        .fetch_all(pool)
        .await?;
    let mut out: Vec<Candidate> = Vec::with_capacity(rows.len());
    for (section_id, decision_id, chunk_id) in rows {
        if !out.iter().any(|c| c.section_id == section_id) {
            out.push(Candidate { chunk_id, ..Candidate::section(section_id, decision_id) });
        }
    }
    Ok(out)
}

pub async fn keyword(pool: &PgPool, q: &CaseQuery, sections: &[i64]) -> Result<Vec<Candidate>, RetrievalError> {
    if sections.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT s.section_id, d.decision_id FROM unnest($7::int8[]) WITH ORDINALITY AS p(section_id, ord) \
         JOIN decision_section s ON s.section_id = p.section_id \
         JOIN decision d ON d.decision_id = s.decision_id \
         JOIN court c ON c.court_id = d.court_id \
         WHERE {filters} ORDER BY p.ord LIMIT $5",
        filters = filters(),
    );
    let rows = bind_filters(sqlx::query_as::<_, (i64, i64)>(AssertSqlSafe(sql)), q)
        .bind(sections)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(s, d)| Candidate::section(s, d)).collect())
}

#[cfg(test)]
mod tests {
    use super::{TEXT_FIRST_SLOT, rare_filter};

    #[test]
    fn rare_filter_points_at_the_chosen_words_slots_only() {
        let sql = rare_filter("s.body", &[0, 3]);
        assert_eq!(sql, format!("(s.body LIKE ${} OR s.body LIKE ${})", TEXT_FIRST_SLOT, TEXT_FIRST_SLOT + 3));
    }
}
