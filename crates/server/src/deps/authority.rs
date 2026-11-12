use engine::{AuthorityHit, AuthoritySearch, AuthoritySnapshot, DepError, Evidence};
use retrieval::{CaseQuery, Corpus, LawQuery, read_law_unit, search_cases, search_laws};
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

pub fn article_of(unit: &str) -> Option<String> {
    let flat: String = unit.chars().filter(|c| !c.is_whitespace()).collect();
    let rest = flat.strip_prefix('제')?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let after = rest[digits.len()..].strip_prefix('조')?;
    if digits.is_empty() {
        return None;
    }
    let branch = after.strip_prefix('의').map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>());
    Some(match branch {
        Some(b) if !b.is_empty() => format!("제{digits}조의{b}"),
        _ => format!("제{digits}조"),
    })
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
    async fn cited(&self, evidence: &[Evidence]) -> Result<AuthoritySnapshot, DepError> {
        let Some(corpus) = self.corpus.as_ref() else {
            return Err(DepError::Unavailable(SEARCH_NOT_CONNECTED.to_string()));
        };
        let mut seen: Vec<(String, String)> = Vec::new();
        for e in evidence {
            if let Some(article) = article_of(&e.unit) {
                let key = (e.law.trim().to_string(), article);
                if !seen.contains(&key) {
                    seen.push(key);
                }
            }
        }
        let mut hits = Vec::new();
        let mut degraded = false;
        for (law, article) in &seen {
            match read_law_unit(corpus.pool(), law, article, None).await {
                Ok(unit) => {
                    let date = unit.version.valid_from.map(|d| format!("(시행 {d})")).unwrap_or_default();
                    hits.push(AuthorityHit {
                        source: "law".into(),
                        citation: format!("{} {}{date}", unit.law_name, unit.unit_key),
                        text: unit.body,
                    });
                }
                Err(e) => {
                    degraded = true;
                    tracing::warn!(law = %law, unit = %article, error = %e, "cited unit could not be read");
                }
            }
        }
        Ok(AuthoritySnapshot { snapshot_hash: snapshot_hash(&hits), hits, degraded })
    }

    async fn search(&self, query: &str, k: usize) -> Result<AuthoritySnapshot, DepError> {
        let Some(corpus) = self.corpus.as_ref() else {
            return Err(DepError::Unavailable(SEARCH_NOT_CONNECTED.to_string()));
        };
        let law_k = k.div_ceil(2).max(1);
        let case_k = (k - law_k.min(k)).max(1);
        let laws = LawQuery { text: query.to_string(), k: law_k, as_of: None, law_type: None, law_ids: None };
        let cases = CaseQuery {
            text: query.to_string(),
            k: case_k,
            field: None,
            court_level: None,
            decided_from: None,
            decided_to: None,
            include_minority: false,
            article_keys: None,
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

#[cfg(test)]
mod tests {
    use super::article_of;

    #[test]
    fn cited_units_are_read_at_article_level() {
        assert_eq!(article_of("제2조제1항").as_deref(), Some("제2조"));
        assert_eq!(article_of(" 제 5 조 제1항 ").as_deref(), Some("제5조"));
        assert_eq!(article_of("제12조의2제3항").as_deref(), Some("제12조의2"));
        assert_eq!(article_of("별표 1"), None);
        assert_eq!(article_of("제조"), None);
    }
}
