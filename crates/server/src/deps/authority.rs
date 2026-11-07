use engine::{AuthoritySearch, AuthoritySnapshot, DepError};

pub const SEARCH_NOT_CONNECTED: &str = "법령·판례 검색이 아직 연결되지 않음";

#[derive(Debug, Clone, Copy, Default)]
pub struct NoAuthority;

impl AuthoritySearch for NoAuthority {
    async fn search(&self, _query: &str, _k: usize) -> Result<AuthoritySnapshot, DepError> {
        Err(DepError::Unavailable(SEARCH_NOT_CONNECTED.to_string()))
    }
}
