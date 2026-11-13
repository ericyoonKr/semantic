use std::collections::HashMap;

use super::{matched, prepare, query_vector, settle};
use crate::corpus::Corpus;
use crate::error::RetrievalError;
use crate::model::{CaseHit, CaseQuery, SearchResult};
use crate::query::cases::candidates::{self, Candidate};
use crate::query::cases::hits::{self, Pick};
use crate::query::cases::{cited, rank};
use crate::query::fusion::{CANDIDATES, RRF_K, rrf};
use crate::query::keyword::error::log_search_failure;

async fn vector_side(corpus: &Corpus, q: &CaseQuery) -> Result<Vec<Candidate>, RetrievalError> {
    if corpus.vector_off() {
        return Err(RetrievalError::VectorOff);
    }
    if !corpus.case_vectors() {
        return Ok(Vec::new());
    }
    let pool = corpus.pool();
    let registry = corpus.embedder().registry_name();
    if !candidates::vector_ready(pool, registry).await? {
        return Err(RetrievalError::VectorEmpty);
    }
    let lit = query_vector(corpus, &q.text).await?;
    candidates::vector(pool, q, registry, &lit).await
}

async fn keyword_side(corpus: &Corpus, q: &CaseQuery) -> Result<Vec<Candidate>, RetrievalError> {
    let Some(index) = corpus.case_keyword() else {
        return Ok(Vec::new());
    };
    let text = q.text.clone();
    let limit = usize::try_from(CANDIDATES).unwrap_or(usize::MAX);
    let found = tokio::task::spawn_blocking(move || index.search_sections(&text, limit)).await;
    let hits = match found {
        Ok(Ok(hits)) => hits,
        Ok(Err(e)) => {
            log_search_failure(&e);
            return Ok(Vec::new());
        }
        Err(e) => {
            tracing::warn!(error = %e, "case keyword search task failed");
            return Ok(Vec::new());
        }
    };
    let sections: Vec<i64> = hits.iter().filter_map(|h| h.id.parse().ok()).collect();
    candidates::keyword(corpus.pool(), q, &sections).await
}

async fn cited_side(corpus: &Corpus, q: &CaseQuery) -> Result<Vec<Candidate>, RetrievalError> {
    let keys = cited::query_keys(corpus.pool(), q).await?;
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let found = cited::citing(corpus.pool(), q, &keys).await?;
    tracing::debug!(keys = keys.len(), decisions = found.len(), "cited-article case branch");
    Ok(found)
}

async fn timed<T>(stage: &'static str, fut: impl std::future::Future<Output = T>) -> T {
    let started = std::time::Instant::now();
    let out = fut.await;
    tracing::info!(stage, elapsed_ms = started.elapsed().as_millis() as u64, "case search stage");
    out
}

const RARE_MIN_CHARS: usize = 3;

fn rare_words(corpus: &Corpus, words: &[String]) -> Vec<usize> {
    let n = corpus.case_text_rare();
    let Some(index) = corpus.case_keyword().filter(|_| n > 0) else {
        return Vec::new();
    };
    match index.rarest_words(words, RARE_MIN_CHARS, n) {
        Ok(found) => found,
        Err(e) => {
            log_search_failure(&e);
            Vec::new()
        }
    }
}

fn ids(list: &[Candidate]) -> Vec<i64> {
    list.iter().map(|c| c.section_id).collect()
}

pub async fn search_cases(corpus: &Corpus, q: &CaseQuery) -> Result<SearchResult<CaseHit>, RetrievalError> {
    let (terms, k) = prepare(&q.text, q.k, corpus.stemmed())?;
    let pool = corpus.pool();
    let rare = rare_words(corpus, terms.words());
    let (text, vector, keyword, cited) = tokio::join!(
        timed("text", candidates::text(pool, q, &terms, &rare)),
        timed("vector", vector_side(corpus, q)),
        timed("keyword", keyword_side(corpus, q)),
        timed("cited", cited_side(corpus, q))
    );
    let (text, keyword, cited) = (text?, keyword?, cited?);
    let (vector, reason) = settle(vector);
    let decision_of: HashMap<i64, i64> = [&text, &vector, &keyword, &cited]
        .into_iter()
        .flatten()
        .map(|c| (c.section_id, c.decision_id))
        .collect();
    let (text_ids, vector_ids, keyword_ids) = (ids(&text), ids(&vector), ids(&keyword));
    let others = rrf(&[&text_ids, &vector_ids, &keyword_ids], RRF_K);
    let cited_ids = cited::place(&cited, &others, &decision_of);
    let sides: [(&str, &[i64]); 4] =
        [("text", &text_ids), ("vector", &vector_ids), ("keyword", &keyword_ids), ("cited", &cited_ids)];
    let lists: Vec<&[i64]> = sides.iter().map(|(_, l)| *l).collect();
    let mut fused = rrf(&lists, RRF_K);
    let all: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();
    let facts = timed("facts", rank::facts(pool, &all)).await?;
    rank::reweigh(&mut fused, &facts, corpus.case_weights());
    fused.truncate(k);
    let positions: HashMap<i64, i32> = text.iter().map(|c| (c.section_id, c.position)).collect();
    let chunks: HashMap<i64, i64> = vector.iter().map(|c| (c.section_id, c.chunk_id)).collect();
    let picks: Vec<Pick> = fused
        .into_iter()
        .map(|(section_id, score)| Pick {
            section_id,
            chunk_id: chunks.get(&section_id).copied().unwrap_or(0),
            position: positions.get(&section_id).copied().unwrap_or(0),
            score,
            matched: matched(&section_id, &sides),
        })
        .collect();
    let hits = timed("hits", hits::hits(pool, &picks)).await?;
    Ok(SearchResult { degraded: reason.is_some(), degraded_reason: reason, scope: Vec::new(), hits })
}
