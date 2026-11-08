use std::collections::HashMap;

use crate::corpus::Corpus;
use crate::embedder::vector_literal;
use crate::error::RetrievalError;
use crate::model::{CaseHit, CaseQuery, LawHit, LawQuery, SearchResult};
use crate::query::cases::{self, Pick};
use crate::query::direct;
use crate::query::fusion::{CANDIDATES, RRF_K, rrf};
use crate::query::laws::{self, UnitRef};
use crate::query::terms::{Terms, compact};

pub const VECTOR_EMPTY: &str = "벡터 칸이 비어 있어 글자 검색만 함";
pub const EMBED_FAILED: &str = "질의 임베딩을 만들지 못해 글자 검색만 함";
pub const VECTOR_FAILED: &str = "벡터 질의가 실패해 글자 검색만 함";
pub const VECTOR_OFF: &str = "벡터 검색을 설정으로 꺼 두어 글자 검색만 함";

fn prepare(text: &str, k: usize) -> Result<(Terms, usize), RetrievalError> {
    if k == 0 {
        return Err(RetrievalError::Argument("k"));
    }
    let terms = Terms::split(text).ok_or(RetrievalError::Argument("query"))?;
    let cap = usize::try_from(CANDIDATES).unwrap_or(usize::MAX);
    Ok((terms, k.min(cap)))
}

fn settle<T>(side: Result<Vec<T>, RetrievalError>) -> (Vec<T>, Option<String>) {
    match side {
        Ok(v) => (v, None),
        Err(e) => {
            let reason = match &e {
                RetrievalError::VectorEmpty => VECTOR_EMPTY,
                RetrievalError::VectorOff => VECTOR_OFF,
                e if e.is_embedding() => EMBED_FAILED,
                _ => VECTOR_FAILED,
            };
            tracing::warn!(reason = %e, "vector search off");
            (Vec::new(), Some(reason.to_string()))
        }
    }
}

async fn query_vector(corpus: &Corpus, text: &str) -> Result<String, RetrievalError> {
    if corpus.vector_off() {
        return Err(RetrievalError::VectorOff);
    }
    let v = corpus.embedder().embed(text).await?;
    vector_literal(&v)
}

fn matched(unit: &UnitRef, text: &[UnitRef], vector: &[UnitRef]) -> String {
    match (text.contains(unit), vector.contains(unit)) {
        (true, true) => "text+vector",
        (false, true) => "vector",
        _ => "text",
    }
    .to_string()
}

pub async fn search_laws(
    corpus: &Corpus,
    q: &LawQuery,
) -> Result<SearchResult<LawHit>, RetrievalError> {
    let parsed = direct::parse(&q.text);
    let words = match &parsed {
        Some(_) => direct::without_numbers(&q.text),
        None => q.text.clone(),
    };
    let (terms, k) = prepare(&words, q.k)?;
    let pool = corpus.pool();
    let registry = corpus.embedder().registry_name();
    let vector_side = async {
        if corpus.vector_off() {
            return Err(RetrievalError::VectorOff);
        }
        if !laws::vector_ready(pool, registry).await? {
            return Err(RetrievalError::VectorEmpty);
        }
        let lit = query_vector(corpus, &q.text).await?;
        laws::vector_candidates(pool, q, registry, &lit).await
    };
    let lookup_only = parsed
        .as_ref()
        .is_some_and(|d| compact(&words) == compact(&d.law));
    let direct_and_text = async {
        let direct = match &parsed {
            Some(d) => laws::direct_candidates(pool, d, q.as_of).await?,
            None => Vec::new(),
        };
        let text = if lookup_only && !direct.is_empty() {
            Vec::new()
        } else {
            laws::text_candidates(pool, q, &terms).await?
        };
        Ok::<_, RetrievalError>((direct, text))
    };
    let (found, vector) = tokio::join!(direct_and_text, vector_side);
    let (direct, text) = found?;
    let (vector, reason) = settle(vector);
    let mut ranked: Vec<(UnitRef, f64, String)> = direct
        .iter()
        .map(|u| (u.clone(), 1.0, "direct".to_string()))
        .collect();
    for (unit, score) in rrf(&[&text, &vector], RRF_K) {
        if ranked.len() >= k {
            break;
        }
        if direct.contains(&unit) {
            continue;
        }
        let how = matched(&unit, &text, &vector);
        ranked.push((unit, score, how));
    }
    ranked.truncate(k);
    let hits = laws::hits(pool, &ranked, q.as_of).await?;
    Ok(SearchResult {
        degraded: reason.is_some(),
        degraded_reason: reason,
        hits,
    })
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
    let (vector, reason) = settle(vector);
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
    Ok(SearchResult {
        degraded: reason.is_some(),
        degraded_reason: reason,
        hits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degraded_reason_names_empty_vectors_apart_from_failures() {
        let (_, r) = settle::<i32>(Err(RetrievalError::VectorEmpty));
        assert_eq!(r.as_deref(), Some(VECTOR_EMPTY));
        let (_, r) = settle::<i32>(Err(RetrievalError::EmbedderDown));
        assert_eq!(r.as_deref(), Some(EMBED_FAILED));
        let (_, r) = settle::<i32>(Err(RetrievalError::Argument("x")));
        assert_eq!(r.as_deref(), Some(VECTOR_FAILED));
        let (_, r) = settle::<i32>(Err(RetrievalError::VectorOff));
        assert_eq!(r.as_deref(), Some(VECTOR_OFF));
        let (v, r) = settle::<i32>(Ok(vec![1]));
        assert_eq!((v, r), (vec![1], None));
    }

    #[test]
    fn matched_says_which_list_found_the_unit() {
        let a = UnitRef::Article(1);
        let n = UnitRef::Node("x".into());
        let one = std::slice::from_ref(&a);
        assert_eq!(matched(&a, one, &[]), "text");
        assert_eq!(matched(&a, one, one), "text+vector");
        assert_eq!(matched(&n, &[], std::slice::from_ref(&n)), "vector");
    }
}
