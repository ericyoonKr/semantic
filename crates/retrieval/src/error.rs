#[derive(Debug, thiserror::Error)]
pub enum RetrievalError {
    #[error("embedding server did not answer")]
    EmbedderDown,
    #[error("embedding server returned status {0}")]
    EmbedHttp(u16),
    #[error("embedding response could not be read")]
    EmbedMalformed,
    #[error("embedding has {0} dimensions, expected 1024")]
    Dimension(usize),
    #[error("no embeddings are loaded")]
    VectorEmpty,
    #[error("corpus database failed: {0}")]
    Db(#[from] sqlx::Error),
    #[error("query rewriting failed: {0}")]
    Rewrite(String),
    #[error("corpus database address is not valid")]
    InvalidUrl,
    #[error("argument is not valid: {0}")]
    Argument(&'static str),
    #[error("{0} not found")]
    NotFound(&'static str),
}

impl RetrievalError {
    pub fn is_embedding(&self) -> bool {
        matches!(
            self,
            Self::EmbedderDown | Self::EmbedHttp(_) | Self::EmbedMalformed | Self::Dimension(_)
        )
    }
}
