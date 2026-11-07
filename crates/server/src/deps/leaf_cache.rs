use engine::{DepError, LeafCache, LeafResult};
use moka::future::Cache;
use sessdb::SessDb;

pub const CACHE_UNREADABLE: &str = "리프 판정 캐시를 읽지 못함";
pub const CACHE_UNWRITABLE: &str = "리프 판정 캐시에 쓰지 못함";

#[derive(Clone)]
pub struct LeafStore {
    mem: Cache<String, LeafResult>,
    db: SessDb,
}

impl LeafStore {
    pub fn new(db: SessDb, entries: u64) -> Self {
        Self {
            mem: Cache::new(entries),
            db,
        }
    }
}

fn failed(what: &'static str, e: impl std::fmt::Display) -> DepError {
    tracing::warn!(error = %e, "leaf cache failed");
    DepError::Other(what.to_string())
}

impl LeafCache for LeafStore {
    async fn get(&self, key: &str) -> Result<Option<LeafResult>, DepError> {
        if let Some(hit) = self.mem.get(key).await {
            return Ok(Some(hit));
        }
        let Some(raw) = self
            .db
            .get_leaf(key.as_bytes())
            .map_err(|e| failed(CACHE_UNREADABLE, e))?
        else {
            return Ok(None);
        };
        match serde_json::from_slice::<LeafResult>(&raw) {
            Ok(value) => {
                self.mem.insert(key.to_string(), value.clone()).await;
                Ok(Some(value))
            }
            Err(e) => {
                tracing::warn!(error = %e, "leaf cache entry could not be read; judged again");
                Ok(None)
            }
        }
    }

    async fn put(&self, key: &str, value: &LeafResult) -> Result<(), DepError> {
        let bytes = serde_json::to_vec(value).map_err(|e| failed(CACHE_UNWRITABLE, e))?;
        self.db
            .put_leaf(key.as_bytes(), &bytes)
            .map_err(|e| failed(CACHE_UNWRITABLE, e))?;
        self.mem.insert(key.to_string(), value.clone()).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use engine::FailureReason;

    use super::*;

    #[tokio::test]
    async fn results_survive_a_new_store_on_the_same_db() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let db = sessdb::open(tmp.path())?;
        let first = LeafStore::new(db.clone(), 8);
        let value = LeafResult::Unknown {
            failure: FailureReason::NoEvidence("x".into()),
        };
        assert_eq!(first.get("k1").await?, None);
        first.put("k1", &value).await?;
        assert_eq!(first.get("k1").await?, Some(value.clone()));
        let second = LeafStore::new(db.clone(), 8);
        assert_eq!(second.get("k1").await?, Some(value));
        db.put_leaf(b"k2", b"not json")?;
        assert_eq!(second.get("k2").await?, None);
        Ok(())
    }
}
