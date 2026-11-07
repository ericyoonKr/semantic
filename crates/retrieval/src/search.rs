use std::collections::HashMap;

use crate::corpus::Corpus;
use crate::embedder::vector_literal;
use crate::error::RetrievalError;
use crate::model::{CaseHit, CaseQuery, LawHit, LawQuery, SearchResult};
use crate::query::cases::{self, Pick};
use crate::query::fusion::{CANDIDATES, RRF_K, rrf};
use crate::query::laws;
use crate::query::terms::Terms;

fn prepare(text: &str, k: usize) -> Result<(Terms, usize), RetrievalError> {
    if k == 0 {
        return Err(RetrievalError::Argument("k"));
    }
    let terms = Terms::split(text).ok_or(RetrievalError::Argument("query"))?;
    let cap = usize::try_from(CANDIDATES).unwrap_or(usize::MAX);
    Ok((terms, k.min(cap)))
}

fn settle<T>(side: Result<Vec<T>, RetrievalError>) -> (Vec<T>, bool) {
    match side {
        Ok(v) => (v, false),
        Err(e) => {
            if e.is_embedding() {
                tracing::warn!(reason = %e, "vector search off: embedding failed");
            } else {
                tracing::warn!(reason = %e, "vector search off: vector query failed");
            }
            (Vec::new(), true)
        }
    }
}

async fn query_vector(corpus: &Corpus, text: &str) -> Result<String, RetrievalError> {
    let v = corpus.embedder().embed(text).await?;
    vector_literal(&v)
}

pub async fn search_laws(
    corpus: &Corpus,
    q: &LawQuery,
) -> Result<SearchResult<LawHit>, RetrievalError> {
    let (terms, k) = prepare(&q.text, q.k)?;
    let pool = corpus.pool();
    let vector_side = async {
        let lit = query_vector(corpus, &q.text).await?;
        laws::vector_candidates(pool, q, corpus.embedder().registry_name(), &lit).await
    };
    let (text, vector) = tokio::join!(laws::text_candidates(pool, q, &terms), vector_side);
    let text = text?;
    let (vector, degraded) = settle(vector);
    let mut fused = rrf(&[&text, &vector], RRF_K);
    fused.truncate(k);
    let hits = laws::hits(pool, &fused).await?;
    Ok(SearchResult { degraded, hits })
}

pub async fn search_cases(
    corpus: &Corpus,
    q: &CaseQuery,
) -> Result<SearchResult<CaseHit>, RetrievalError> {
    let (terms, k) = prepare(&q.text, q.k)?;
    let pool = corpus.pool();
    let vector_side = async {
        let lit = query_vector(corpus, &q.text).await?;
        cases::vector_candidates(pool, q, corpus.embedder().registry_name(), &lit).await
    };
    let (text, vector) = tokio::join!(cases::text_candidates(pool, q, &terms), vector_side);
    let text = text?;
    let (vector, degraded) = settle(vector);
    let text_ids: Vec<i64> = text.iter().map(|c| c.section_id).collect();
    let vector_ids: Vec<i64> = vector.iter().map(|c| c.section_id).collect();
    let positions: HashMap<i64, i32> = text.iter().map(|c| (c.section_id, c.position)).collect();
    let chunks: HashMap<i64, i64> = vector.iter().map(|c| (c.section_id, c.chunk_id)).collect();
    let mut fused = rrf(&[&text_ids, &vector_ids], RRF_K);
    fused.truncate(k);
    let picks: Vec<Pick> = fused
        .into_iter()
        .map(|(section_id, score)| Pick {
            section_id,
            chunk_id: chunks.get(&section_id).copied().unwrap_or(0),
            position: positions.get(&section_id).copied().unwrap_or(0),
            score,
        })
        .collect();
    let hits = cases::hits(pool, &picks).await?;
    Ok(SearchResult { degraded, hits })
}
