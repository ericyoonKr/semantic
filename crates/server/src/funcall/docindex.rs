use std::time::Instant;

use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use sessdb::{Part, doc_key};

use super::docs::session_docs;
use super::next_step::Doc;
use super::docsearch::{Passage, decode_vectors, encode_vectors, fuse, keyword_scores, passages, vector_scores};
use crate::state::AppState;

const EMBED_CONCURRENCY: usize = 8;
const DEPTH: usize = 50;
pub const DEFAULT_K: usize = 6;
pub const MAX_K: usize = 20;

#[derive(Debug, Deserialize)]
pub struct DocSearchRequest {
    pub query: String,
    pub k: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct DocHit {
    pub file_id: String,
    pub filename: String,
    pub page: Option<u32>,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct DocSearchReply {
    pub hits: Vec<DocHit>,
    pub processing: Vec<String>,
    pub by_meaning: bool,
}

async fn docs_of(state: &AppState, session: &str) -> Vec<Doc> {
    let db = state.db.clone();
    let session = session.to_string();
    tokio::task::spawn_blocking(move || session_docs(&db, &session)).await.unwrap_or_default()
}

fn vectors_of(state: &AppState, session: &str, file_id: &str, count: usize) -> Option<Vec<Vec<f32>>> {
    let key = doc_key(session, file_id, Part::Vectors).ok()?;
    decode_vectors(&state.db.get_state(&key).ok()??, count)
}

pub async fn build_index(state: &AppState, session: &str, file_id: &str) {
    let Some(corpus) = state.corpus.clone() else { return };
    let Some(doc) = docs_of(state, session).await.into_iter().find(|d| d.file_id == file_id) else { return };
    let parts = passages(&doc.text);
    if parts.is_empty() {
        return;
    }
    let started = Instant::now();
    let jobs: Vec<_> = parts
        .iter()
        .map(|p| {
            let corpus = corpus.clone();
            let text = p.text.clone();
            async move { corpus.embedder().embed(&text).await.ok() }
        })
        .collect();
    let embedded: Vec<Option<Vec<f32>>> = stream::iter(jobs).buffered(EMBED_CONCURRENCY).collect().await;
    let Some(vectors) = embedded.into_iter().collect::<Option<Vec<_>>>() else {
        tracing::warn!(file_id, "doc.index_embed_failed");
        return;
    };
    let Ok(key) = doc_key(session, file_id, Part::Vectors) else { return };
    let meta = doc_key(session, file_id, Part::Meta).ok().and_then(|k| state.db.get_state(&k).ok().flatten());
    if meta.is_none() {
        return;
    }
    match state.db.put_state(&key, &encode_vectors(&vectors)) {
        Ok(()) => tracing::info!(file_id, passages = vectors.len(), elapsed_ms = started.elapsed().as_millis() as u64, "doc.indexed"),
        Err(error) => tracing::warn!(%error, "doc.index_store_failed"),
    }
}

pub async fn search(state: &AppState, session: &str, req: &DocSearchRequest) -> DocSearchReply {
    let docs = docs_of(state, session).await;
    let processing = docs.iter().filter(|d| d.processing).map(|d| d.filename.clone()).collect();
    let mut all: Vec<(usize, Passage)> = Vec::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for (i, doc) in docs.iter().enumerate() {
        let start = all.len();
        all.extend(passages(&doc.text).into_iter().map(|p| (i, p)));
        spans.push((start, all.len()));
    }
    let flat: Vec<Passage> = all.iter().map(|(_, p)| p.clone()).collect();
    let mut lists = vec![keyword_scores(&req.query, &flat)];
    let mut by_meaning = false;
    if let Some(corpus) = state.corpus.as_ref()
        && let Ok(query) = corpus.embedder().embed(&req.query).await
    {
        let mut scored = Vec::new();
        for (doc, &(start, end)) in docs.iter().zip(&spans) {
            if let Some(vectors) = vectors_of(state, session, &doc.file_id, end - start) {
                scored.extend(vector_scores(&query, &vectors).into_iter().map(|(i, s)| (start + i, s)));
            }
        }
        by_meaning = !scored.is_empty();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        lists.push(scored);
    }
    let k = req.k.unwrap_or(DEFAULT_K).clamp(1, MAX_K);
    let hits = fuse(&lists, DEPTH)
        .into_iter()
        .take(k)
        .filter_map(|(i, _)| {
            let (d, p) = all.get(i)?;
            let doc = docs.get(*d)?;
            Some(DocHit { file_id: doc.file_id.clone(), filename: doc.filename.clone(), page: p.page, text: p.text.clone() })
        })
        .collect();
    DocSearchReply { hits, processing, by_meaning }
}
