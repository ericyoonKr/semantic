use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::embedder::Embedder;
use crate::error::RetrievalError;
use crate::query::cases::rank::CaseWeights;
use crate::query::keyword::KeywordIndex;
use crate::query::weights::LawWeights;

pub const HNSW_ITERATIVE_SCAN: &str = "strict_order";
pub const HNSW_EF_SEARCH: &str = "200";

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
            ("hnsw.iterative_scan", HNSW_ITERATIVE_SCAN),
            ("hnsw.ef_search", HNSW_EF_SEARCH),
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
    weights: LawWeights,
    stemmed: bool,
    law_scope: bool,
    unit_vectors: bool,
    case_vectors: bool,
    case_text_rare: usize,
    keyword: Option<Arc<KeywordIndex>>,
    case_keyword: Option<Arc<KeywordIndex>>,
    case_weights: CaseWeights,
}

impl fmt::Debug for Corpus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Corpus").finish_non_exhaustive()
    }
}

impl Corpus {
    pub fn new(pool: PgPool, embedder: Arc<dyn Embedder>) -> Self {
        Self {
            pool,
            embedder,
            vector_off: false,
            weights: LawWeights::default(),
            stemmed: false,
            law_scope: true,
            unit_vectors: false,
            case_vectors: true,
            case_text_rare: 0,
            keyword: None,
            case_keyword: None,
            case_weights: CaseWeights::default(),
        }
    }

    pub fn with_weights(mut self, weights: LawWeights) -> Self {
        self.weights = weights;
        self
    }

    pub fn with_stemming(mut self, on: bool) -> Self {
        self.stemmed = on;
        self
    }

    pub fn with_law_scope(mut self, on: bool) -> Self {
        self.law_scope = on;
        self
    }

    pub fn law_scope(&self) -> bool {
        self.law_scope
    }

    pub fn with_unit_vectors(mut self, on: bool) -> Self {
        self.unit_vectors = on;
        self
    }

    pub fn unit_vectors(&self) -> bool {
        self.unit_vectors
    }

    pub fn with_case_vectors(mut self, on: bool) -> Self {
        self.case_vectors = on;
        self
    }

    pub fn case_vectors(&self) -> bool {
        self.case_vectors
    }

    pub fn with_case_text_rare(mut self, n: usize) -> Self {
        self.case_text_rare = n;
        self
    }

    pub fn case_text_rare(&self) -> usize {
        self.case_text_rare
    }

    pub fn with_keyword(mut self, index: KeywordIndex) -> Self {
        self.keyword = Some(Arc::new(index));
        self
    }

    pub fn keyword(&self) -> Option<Arc<KeywordIndex>> {
        self.keyword.clone()
    }

    pub fn with_case_keyword(mut self, index: KeywordIndex) -> Self {
        self.case_keyword = Some(Arc::new(index));
        self
    }

    pub fn case_keyword(&self) -> Option<Arc<KeywordIndex>> {
        self.case_keyword.clone()
    }

    pub fn with_case_weights(mut self, weights: CaseWeights) -> Self {
        self.case_weights = weights;
        self
    }

    pub fn case_weights(&self) -> &CaseWeights {
        &self.case_weights
    }

    pub fn stemmed(&self) -> bool {
        self.stemmed
    }

    pub fn weights(&self) -> &LawWeights {
        &self.weights
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
