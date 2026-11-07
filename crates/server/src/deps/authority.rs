use engine::{AuthorityHit, AuthoritySearch, AuthoritySnapshot, DepError};
use retrieval::{CaseQuery, Corpus, LawQuery, search_cases, search_laws};
use sha2::{Digest, Sha256};

pub const SEARCH_NOT_CONNECTED: &str = "법령·판례 검색이 아직 연결되지 않음";
pub const SEARCH_FAILED: &str = "법령·판례 검색 실패";

#[derive(Clone, Default)]
pub struct CorpusAuthority {
    corpus: Option<Corpus>,
}

impl CorpusAuthority {
    pub fn new(corpus: Option<Corpus>) -> Self {
        Self { corpus }
    }
}

fn snapshot_hash(hits: &[AuthorityHit]) -> String {
    let mut h = Sha256::new();
    for hit in hits {
        for part in [&hit.source, &hit.text, &hit.citation] {
            h.update(part.as_bytes());
            h.update([0u8]);
        }
        h.update(b"\n");
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

impl AuthoritySearch for CorpusAuthority {
    async fn search(&self, query: &str, k: usize) -> Result<AuthoritySnapshot, DepError> {
        let Some(corpus) = self.corpus.as_ref() else {
            return Err(DepError::Unavailable(SEARCH_NOT_CONNECTED.to_string()));
        };
        let law_k = k.div_ceil(2).max(1);
        let case_k = (k - law_k.min(k)).max(1);
        let laws = LawQuery { text: query.to_string(), k: law_k, as_of: None, law_type: None };
        let cases = CaseQuery {
            text: query.to_string(),
            k: case_k,
            field: None,
            court_level: None,
            decided_from: None,
            decided_to: None,
        };
        let (laws, cases) = tokio::join!(search_laws(corpus, &laws), search_cases(corpus, &cases));
        if let (Err(a), Err(b)) = (&laws, &cases) {
            tracing::warn!(laws = %a, cases = %b, "authority search failed");
            return Err(DepError::Unavailable(SEARCH_FAILED.to_string()));
        }
        let mut degraded = laws.is_err() || cases.is_err();
        let mut hits = Vec::new();
        if let Ok(r) = laws {
            degraded |= r.degraded;
            hits.extend(r.hits.into_iter().map(|h| {
                let date = h.enforcement_date.map(|d| format!("(시행 {d})")).unwrap_or_default();
                AuthorityHit {
                    source: "law".into(),
                    citation: format!("{} {}{date}", h.law_name, h.unit_key),
                    text: h.text,
                }
            }));
        }
        if let Ok(r) = cases {
            degraded |= r.degraded;
            hits.extend(r.hits.into_iter().map(|h| AuthorityHit {
                source: "case".into(),
                citation: format!("{} {} 판결(선고 {})", h.court, h.case_no, h.decided_on),
                text: h.text,
            }));
        }
        hits.truncate(k.max(1));
        Ok(AuthoritySnapshot { snapshot_hash: snapshot_hash(&hits), hits, degraded })
    }
}
