pub mod corpus;
pub mod embed_client;
pub mod embedder;
pub mod error;
pub mod model;
pub mod query;
pub mod search;

pub use corpus::{Corpus, CorpusDb};
pub use embed_client::{EmbedClient, EmbedSettings};
pub use embedder::{DIM, EmbedFuture, Embedder, NoEmbedder};
pub use error::RetrievalError;
pub use model::{
    CaseDoc, CaseHit, CaseQuery, CaseSection, CitationItem, Citations, CitationsBy, CitedUnit,
    CitingCase, LawHit, LawQuery, LawUnit, LawVersion, SearchResult,
};
pub use query::cases::read_case;
pub use query::citations::citations;
pub use query::laws::read_law_unit;
pub use search::{search_cases, search_laws};
