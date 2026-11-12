use std::collections::{HashMap, HashSet};

use sqlx::{AssertSqlSafe, PgPool};

use super::candidates::Candidate;
use super::{CITATION_RUN, DECISION_FILTERS, bind_filters, opinion_allowed};
use crate::error::RetrievalError;
use crate::model::CaseQuery;
use crate::query::direct::{self, Target};
use crate::query::laws::resolve_law;

const ARTICLE_KEYS: &str = "SELECT DISTINCT av.article_key FROM article_version av \
     WHERE av.law_id = $1 AND av.article_no = $2 AND upper_inf(av.sys_period) ORDER BY av.article_key";

pub async fn article_keys_of(pool: &PgPool, law_id: &str, article_no: &str) -> Result<Vec<String>, RetrievalError> {
    Ok(sqlx::query_scalar(ARTICLE_KEYS).bind(law_id).bind(article_no).fetch_all(pool).await?)
}

pub async fn query_keys(pool: &PgPool, q: &CaseQuery) -> Result<Vec<String>, RetrievalError> {
    let mut keys: Vec<String> = q
        .article_keys
        .iter()
        .flatten()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect();
    if let Some(d) = direct::parse(&q.text)
        && let Target::Article(no) = &d.target
        && let Some((law_id, _, _)) = resolve_law(pool, &d.law).await?
    {
        keys.extend(article_keys_of(pool, &law_id, no).await?);
    }
    let mut seen = HashSet::new();
    keys.retain(|k| seen.insert(k.clone()));
    Ok(keys)
}

fn citing_sql() -> String {
    format!(
        "WITH hit AS ( \
           SELECT dc.decision_id, count(DISTINCT dc.article_key) AS n FROM decision_citation dc \
           WHERE dc.run_id IN {CITATION_RUN} AND dc.article_key = ANY($7::text[]) \
           GROUP BY dc.decision_id) \
         SELECT d.decision_id, rep.section_id FROM hit \
         JOIN decision d ON d.decision_id = hit.decision_id \
         JOIN court c ON c.court_id = d.court_id \
         JOIN LATERAL ( \
           SELECT s.section_id FROM decision_section s \
           WHERE s.decision_id = d.decision_id AND {opinion} \
           ORDER BY CASE s.role WHEN 'holding_summary' THEN 0 WHEN 'holding_issue' THEN 1 WHEN 'reasons' THEN 2 ELSE 3 END, s.ord \
           LIMIT 1) rep ON true \
         WHERE {DECISION_FILTERS} \
         ORDER BY hit.n DESC, d.decided_on DESC, d.decision_id LIMIT $5",
        opinion = opinion_allowed(6),
    )
}

pub async fn citing(pool: &PgPool, q: &CaseQuery, keys: &[String]) -> Result<Vec<Candidate>, RetrievalError> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let rows = bind_filters(sqlx::query_as::<_, (i64, i64)>(AssertSqlSafe(citing_sql())), q)
        .bind(keys)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(d, s)| Candidate::section(s, d)).collect())
}

pub fn place(cited: &[Candidate], others: &[(i64, f64)], decision_of: &HashMap<i64, i64>) -> Vec<i64> {
    let mut best: HashMap<i64, i64> = HashMap::new();
    for (section, _) in others {
        if let Some(d) = decision_of.get(section) {
            best.entry(*d).or_insert(*section);
        }
    }
    cited
        .iter()
        .map(|c| best.get(&c.decision_id).copied().unwrap_or(c.section_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cited_decision_takes_its_best_section_from_the_other_lists() {
        let cited = [Candidate::section(900, 9), Candidate::section(800, 8)];
        let others = [(71, 0.03), (90, 0.02), (91, 0.01)];
        let decision_of: HashMap<i64, i64> = [(71, 7), (90, 9), (91, 9)].into_iter().collect();
        assert_eq!(place(&cited, &others, &decision_of), vec![90, 800]);
        assert_eq!(place(&[], &others, &decision_of), Vec::<i64>::new());
    }
}
