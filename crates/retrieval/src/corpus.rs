use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::embedder::Embedder;
use crate::error::RetrievalError;

#[derive(Clone)]
pub struct CorpusDb(PgConnectOptions);

impl fmt::Debug for CorpusDb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CorpusDb(***)")
    }
}

impl CorpusDb {
    pub fn parse(url: &str) -> Result<Self, RetrievalError> {
        PgConnectOptions::from_str(url)
            .map(Self)
            .map_err(|_| RetrievalError::InvalidUrl)
    }

    pub fn connect_lazy(&self, max_connections: u32, statement_timeout: Duration) -> PgPool {
        let millis = statement_timeout.as_millis().to_string();
        let options = self.0.clone().options([
            ("default_transaction_read_only", "on"),
            ("statement_timeout", millis.as_str()),
        ]);
        PgPoolOptions::new()
            .max_connections(max_connections.max(1))
            .acquire_timeout(statement_timeout)
            .connect_lazy_with(options)
    }
}

#[derive(Clone)]
pub struct Corpus {
    pool: PgPool,
    embedder: Arc<dyn Embedder>,
    vector_off: bool,
}

impl fmt::Debug for Corpus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Corpus").finish_non_exhaustive()
    }
}

impl Corpus {
    pub fn new(pool: PgPool, embedder: Arc<dyn Embedder>) -> Self {
        Self { pool, embedder, vector_off: false }
    }

    pub fn without_vectors(mut self) -> Self {
        self.vector_off = true;
        self
    }

    pub fn vector_off(&self) -> bool {
        self.vector_off
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn embedder(&self) -> &dyn Embedder {
        self.embedder.as_ref()
    }
}
