pub mod corpus;
pub mod embed_client;
pub mod embed_vertex;
pub mod embedder;
pub mod error;
pub mod model;
pub mod query;
pub mod search;

pub use corpus::{CasePhrase, Corpus, CorpusDb};
pub use embed_client::{EmbedClient, EmbedSettings};
pub use embedder::{DIM, EmbedFuture, Embedder, NoEmbedder};
pub use error::RetrievalError;
pub use model::{
    AnnexCompute, AnnexLookup, AnnexPara, AnnexRow, AnnexRuleItem, AnnexTable, AnnexVersion, CaseDoc,
    CaseHit, CaseQuery, CaseSection, CitationItem, Citations, CitationsBy, CitedUnit, CitingCase,
    Computation, Computed, LawHit, LawQuery, LawUnit, LawVersion, RelatedArticle, SearchResult,
};
pub use query::annex::{LOOKUP_DEFAULT, LOOKUP_MAX, annex_compute, annex_lookup};
pub use query::annex_list::{AnnexEntry, AnnexList, list_annexes};
pub use query::cases::rank::CaseWeights;
pub use query::cases::read_case;
pub use query::citations::citations;
pub use query::govern::named_laws;
pub use query::laws::read_law_unit;
pub use query::weights::LawWeights;
pub use search::{search_cases, search_laws};
