use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use kg::ShortIds;
use sessdb::{SessDb, kg_ids_key};
use tokio::sync::{Mutex, MutexGuard};

use crate::error::ApiError;
use crate::session::SessionId;

const STRIPES: usize = 64;

pub struct KgIdLocks {
    stripes: Vec<Mutex<()>>,
}

impl Default for KgIdLocks {
    fn default() -> Self {
        Self {
            stripes: (0..STRIPES).map(|_| Mutex::new(())).collect(),
        }
    }
}

impl KgIdLocks {
    pub async fn lock(&self, sid: &SessionId) -> Option<MutexGuard<'_, ()>> {
        let mut h = DefaultHasher::new();
        sid.as_str().hash(&mut h);
        let i = usize::try_from(h.finish() % STRIPES as u64).unwrap_or(0);
        match self.stripes.get(i) {
            Some(m) => Some(m.lock().await),
            None => None,
        }
    }
}

pub fn load(db: &SessDb, sid: &SessionId) -> Result<ShortIds, ApiError> {
    let raw = db.get_state(&kg_ids_key(sid.as_str())?)?;
    Ok(raw
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default())
}

pub fn save(db: &SessDb, sid: &SessionId, ids: &ShortIds) -> Result<(), ApiError> {
    let bytes = serde_json::to_vec(ids).map_err(|_| ApiError::internal())?;
    db.put_state(&kg_ids_key(sid.as_str())?, &bytes)?;
    Ok(())
}
