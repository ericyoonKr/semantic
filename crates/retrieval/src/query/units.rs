use sqlx::PgPool;

use crate::error::RetrievalError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitHit {
    pub article_row: i64,
    pub unit_id: i64,
}

const NEAREST_IN_ARTICLES: &str = "SELECT article_row_id, unit_id FROM ( \
       SELECT u.article_row_id, u.unit_id, \
              row_number() OVER (PARTITION BY u.article_row_id \
                                 ORDER BY u.embedding <=> CAST($3::text AS halfvec), u.unit_id) AS rn \
       FROM article_unit u JOIN embedding_model m ON m.model_id = u.embed_model_id \
       WHERE u.article_row_id = ANY($1) AND m.name = $2 AND u.embedding IS NOT NULL) t \
     WHERE rn <= $4 ORDER BY article_row_id, rn";

pub async fn nearest_in_articles(
    pool: &PgPool,
    rows: &[i64],
    registry: &str,
    vector: &str,
    per_article: usize,
) -> Result<Vec<UnitHit>, RetrievalError> {
    let limit = i64::try_from(per_article).unwrap_or(i64::MAX);
    let rows: Vec<(i64, i64)> = sqlx::query_as(NEAREST_IN_ARTICLES)
        .bind(rows)
        .bind(registry)
        .bind(vector)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(article_row, unit_id)| UnitHit { article_row, unit_id }).collect())
}

#[cfg(test)]
mod tests {
    use super::NEAREST_IN_ARTICLES;

    #[test]
    fn nearest_units_are_ranked_only_inside_the_given_articles() {
        assert!(NEAREST_IN_ARTICLES.contains("u.article_row_id = ANY($1)"));
        assert!(NEAREST_IN_ARTICLES.contains("PARTITION BY u.article_row_id"));
        assert!(NEAREST_IN_ARTICLES.contains("rn <= $4"));
    }
}
