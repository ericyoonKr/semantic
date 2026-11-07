#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::NaiveDate;
use retrieval::{Corpus, CorpusDb, DIM, EmbedFuture, Embedder, LawQuery, RetrievalError};

use crate::fixture::{Checked, Fixture, check};

pub struct FixedEmbedder {
    pub calls: AtomicUsize,
}

impl Embedder for FixedEmbedder {
    fn registry_name(&self) -> &str {
        "registry-model"
    }

    fn embed<'a>(&'a self, _text: &'a str) -> EmbedFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(vec![0.01_f32; DIM]) })
    }
}

pub fn corpus(fx: &Fixture, embedder: Arc<dyn Embedder>) -> Result<Corpus, RetrievalError> {
    let pool = CorpusDb::parse(&fx.url)?.connect_lazy(4, Duration::from_secs(10));
    Ok(Corpus::new(pool, embedder))
}

pub fn day(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y, m, d)
}

pub async fn with_fixture<F, Fut>(body: F) -> Checked
where
    F: FnOnce(Fixture) -> Fut,
    Fut: Future<Output = (Fixture, Checked)>,
{
    let Some(fx) = crate::fixture::open().await? else {
        return Ok(());
    };
    let (fx, result) = body(fx).await;
    let schema = fx.schema.clone();
    fx.close().await?;
    if let Ok(base) = std::env::var(crate::fixture::URL_VAR) {
        check(
            !Fixture::schema_exists(&base, &schema).await?,
            "temporary schema was not dropped",
        )?;
    }
    result
}

pub fn law_query(text: &str) -> LawQuery {
    LawQuery {
        text: text.into(),
        k: 8,
        ..LawQuery::default()
    }
}

