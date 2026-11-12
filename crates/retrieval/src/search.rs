use std::collections::HashMap;

use crate::corpus::Corpus;
use crate::embedder::vector_literal;
use crate::error::RetrievalError;
use crate::model::{CaseHit, CaseQuery, LawHit, LawQuery, SearchResult};
use crate::query::cases::{self, Pick};
use crate::query::direct;
use crate::query::govern::{self, Scope};
use crate::query::fusion::{CANDIDATES, RRF_K, rrf};
use crate::query::keyword::{KIND_NODE, KIND_UNIT, error::log_search_failure};
use crate::query::weights;
use crate::query::laws::{self, UnitRef};
use crate::query::terms::{Terms, compact};

pub const VECTOR_EMPTY: &str = "벡터 칸이 비어 있어 글자 검색만 함";
pub const EMBED_FAILED: &str = "질의 임베딩을 만들지 못해 글자 검색만 함";
pub const VECTOR_FAILED: &str = "벡터 질의가 실패해 글자 검색만 함";
const MATCHED_UNITS: usize = 3;
pub const VECTOR_OFF: &str = "벡터 검색을 설정으로 꺼 두어 글자 검색만 함";

fn prepare(text: &str, k: usize, stemmed: bool) -> Result<(Terms, usize), RetrievalError> {
    if k == 0 {
        return Err(RetrievalError::Argument("k"));
    }
    let terms = Terms::split_with(text, stemmed).ok_or(RetrievalError::Argument("query"))?;
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

fn matched(unit: &UnitRef, sides: &[(&str, &[UnitRef])]) -> String {
    let parts: Vec<&str> = sides.iter().filter(|(_, side)| side.contains(unit)).map(|(name, _)| *name).collect();
    if parts.is_empty() { "text".to_string() } else { parts.join("+") }
}

#[derive(Debug, Default)]
struct KeywordSide {
    refs: Vec<UnitRef>,
    units: HashMap<i64, Vec<i64>>,
}

impl KeywordSide {
    fn add(&mut self, unit: UnitRef, unit_id: Option<i64>) {
        if let (UnitRef::Article(row), Some(id)) = (&unit, unit_id) {
            let ids = self.units.entry(*row).or_default();
            if ids.len() < MATCHED_UNITS && !ids.contains(&id) {
                ids.push(id);
            }
        }
        if !self.refs.contains(&unit) {
            self.refs.push(unit);
        }
    }
}

async fn matched_paths(pool: &sqlx::PgPool, sides: &[&KeywordSide]) -> Result<HashMap<i64, Vec<String>>, RetrievalError> {
    let mut by_row: HashMap<i64, Vec<i64>> = HashMap::new();
    for side in sides {
        for (row, ids) in &side.units {
            let all = by_row.entry(*row).or_default();
            for id in ids {
                if all.len() < MATCHED_UNITS && !all.contains(id) {
                    all.push(*id);
                }
            }
        }
    }
    let ids: Vec<i64> = by_row.values().flatten().copied().collect();
    let paths = laws::unit_paths(pool, &ids).await?;
    Ok(by_row
        .into_iter()
        .map(|(row, ids)| (row, ids.iter().filter_map(|i| paths.get(i).cloned()).collect()))
        .collect())
}

async fn keyword_candidates(corpus: &Corpus, q: &LawQuery, law_ids: Option<&[String]>) -> KeywordSide {
    let mut side = KeywordSide::default();
    let Some(index) = corpus.keyword() else {
        return side;
    };
    let on = q.as_of.unwrap_or_else(|| chrono::Local::now().date_naive());
    let text = q.text.clone();
    let ids: Vec<String> = law_ids.map(<[String]>::to_vec).unwrap_or_default();
    let limit = usize::try_from(CANDIDATES).unwrap_or(usize::MAX);
    let found = tokio::task::spawn_blocking(move || index.search(&text, &ids, on, limit)).await;
    let hits = match found {
        Ok(Ok(hits)) => hits,
        Ok(Err(e)) => {
            log_search_failure(&e);
            return side;
        }
        Err(e) => {
            tracing::warn!(error = %e, "keyword search task failed");
            return side;
        }
    };
    for h in hits {
        match h.kind.as_str() {
            KIND_UNIT => side.add(UnitRef::Article(h.row), h.id.parse().ok()),
            KIND_NODE => side.add(UnitRef::Node(h.id.clone()), None),
            _ => {}
        }
    }
    side
}

async fn scope_of(corpus: &Corpus, q: &LawQuery) -> Result<Scope, RetrievalError> {
    if let Some(ids) = &q.law_ids {
        return Ok(Scope::Only(ids.clone()));
    }
    if !corpus.law_scope() {
        return Ok(Scope::All);
    }
    let ids = govern::governing_laws(corpus.pool(), &q.text).await?;
    Ok(if ids.is_empty() { Scope::All } else { Scope::Prefer(ids) })
}

async fn in_law_candidates(
    corpus: &Corpus,
    q: &LawQuery,
    terms: &Terms,
    scope: &Scope,
) -> Result<(Vec<UnitRef>, KeywordSide), RetrievalError> {
    let Some(ids) = scope.preferred() else {
        return Ok((Vec::new(), KeywordSide::default()));
    };
    let (text, keyword) = tokio::join!(
        laws::text_candidates(corpus.pool(), q, terms, corpus.weights(), Some(ids)),
        keyword_candidates(corpus, q, Some(ids))
    );
    Ok((text?, keyword))
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
    let (terms, k) = prepare(&words, q.k, corpus.stemmed())?;
    let pool = corpus.pool();
    let registry = corpus.embedder().registry_name();
    let scope = scope_of(corpus, q).await?;
    let only = scope.only();
    let vector_side = async {
        if corpus.vector_off() {
            return Err(RetrievalError::VectorOff);
        }
        if !laws::vector_ready(pool, registry).await? {
            return Err(RetrievalError::VectorEmpty);
        }
        let lit = query_vector(corpus, &q.text).await?;
        laws::vector_candidates(pool, q, registry, &lit, only).await
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
            laws::text_candidates(pool, q, &terms, corpus.weights(), only).await?
        };
        Ok::<_, RetrievalError>((direct, text))
    };
    let (found, vector, keyword, in_law) = tokio::join!(
        direct_and_text,
        vector_side,
        keyword_candidates(corpus, q, only),
        in_law_candidates(corpus, q, &terms, &scope)
    );
    let (direct, text) = found?;
    let (text_in_law, keyword_in_law) = in_law?;
    let (vector, reason) = settle(vector);
    let mut ranked: Vec<(UnitRef, f64, String)> = direct
        .iter()
        .map(|u| (u.clone(), 1.0, "direct".to_string()))
        .collect();
    let sides: [(&str, &[UnitRef]); 5] = [
        ("text", &text),
        ("vector", &vector),
        ("keyword", &keyword.refs),
        ("text-in-law", &text_in_law),
        ("keyword-in-law", &keyword_in_law.refs),
    ];
    let lists: Vec<&[UnitRef]> = sides.iter().map(|(_, l)| *l).collect();
    let mut fused = rrf(&lists, RRF_K);
    if !corpus.weights().is_flat() {
        let units: Vec<UnitRef> = fused.iter().map(|(u, _)| u.clone()).collect();
        let w = weights::unit_weights(pool, &units, corpus.weights()).await?;
        for (u, s) in fused.iter_mut() {
            *s *= w.get(u).copied().unwrap_or(1.0);
        }
        fused.sort_by(|a, b| b.1.total_cmp(&a.1));
    }
    for (unit, score) in fused {
        if ranked.len() >= k {
            break;
        }
        if direct.contains(&unit) {
            continue;
        }
        let how = matched(&unit, &sides);
        ranked.push((unit, score, how));
    }
    ranked.truncate(k);
    let units = matched_paths(pool, &[&keyword, &keyword_in_law]).await?;
    let hits = laws::hits(pool, &ranked, q.as_of, &units).await?;
    Ok(SearchResult {
        degraded: reason.is_some(),
        degraded_reason: reason,
        scope: scope.ids().to_vec(),
        hits,
    })
}

pub async fn search_cases(
    corpus: &Corpus,
    q: &CaseQuery,
) -> Result<SearchResult<CaseHit>, RetrievalError> {
    let (terms, k) = prepare(&q.text, q.k, corpus.stemmed())?;
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
        scope: Vec::new(),
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
        let nn = std::slice::from_ref(&n);
        assert_eq!(matched(&a, &[("text", one), ("vector", &[])]), "text");
        assert_eq!(matched(&a, &[("text", one), ("vector", one)]), "text+vector");
        assert_eq!(matched(&n, &[("text", &[]), ("vector", nn)]), "vector");
        assert_eq!(matched(&a, &[("text", &[]), ("keyword", one), ("text-in-law", one)]), "keyword+text-in-law");
        assert_eq!(matched(&a, &[("text", &[]), ("vector", nn)]), "text");
    }
}
