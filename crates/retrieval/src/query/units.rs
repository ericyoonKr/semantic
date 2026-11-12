use sqlx::{AssertSqlSafe, PgPool};

use crate::error::RetrievalError;
use crate::model::LawQuery;
use crate::query::fusion::CANDIDATES;
use crate::query::laws::{OPEN_ARTICLE, VALID_AV};

const LAW_TYPE: &str = "($2::text[] IS NULL OR l.law_type = ANY($2::text[]))";
const LAW_IDS: &str = "($5::text[] IS NULL OR l.law_id = ANY($5::text[]))";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitHit {
    pub article_row: i64,
    pub unit_id: i64,
}

fn vector_sql() -> String {
    format!(
        "SELECT u.article_row_id, u.unit_id FROM article_unit u \
         JOIN embedding_model m ON m.model_id = u.embed_model_id \
         JOIN article_version av ON av.row_id = u.article_row_id \
         JOIN law l ON l.law_id = av.law_id \
         WHERE m.name = $3 AND u.embedding IS NOT NULL AND {OPEN_ARTICLE} AND {VALID_AV} AND {LAW_TYPE} AND {LAW_IDS} \
         ORDER BY u.embedding <=> CAST($4::text AS halfvec) LIMIT $6"
    )
}

pub async fn vector_candidates(
    pool: &PgPool,
    q: &LawQuery,
    registry: &str,
    vector: &str,
    law_ids: Option<&[String]>,
) -> Result<Vec<UnitHit>, RetrievalError> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(AssertSqlSafe(vector_sql()))
        .bind(q.as_of)
        .bind(q.law_type.clone())
        .bind(registry)
        .bind(vector)
        .bind(law_ids)
        .bind(CANDIDATES)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(article_row, unit_id)| UnitHit { article_row, unit_id }).collect())
}

#[cfg(test)]
mod tests {
    use super::vector_sql;

    #[test]
    fn unit_vector_query_uses_the_partial_hnsw_index_condition_and_every_bound_slot() {
        let sql = vector_sql();
        assert!(sql.contains("u.embedding IS NOT NULL"));
        assert!(sql.contains("ORDER BY u.embedding <=> CAST($4::text AS halfvec)"));
        for slot in ["$1", "$2", "$3", "$4", "$5", "$6"] {
            assert!(sql.contains(slot), "{slot}");
        }
    }
}
