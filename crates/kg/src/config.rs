use std::fmt;
use std::sync::Arc;

use crate::verify::{SignatureVerifier, UncheckedSignature};

pub const DEFAULT_MMAP_BYTES: i64 = 64 * 1024 * 1024;
pub const DEFAULT_CACHE_KIB: i64 = 8 * 1024;

#[derive(Clone)]
pub struct KgConfig {
    pub consumer_schema: String,
    pub manifest_schema: String,
    pub mmap_bytes: i64,
    pub cache_kib: i64,
    pub signature: Arc<dyn SignatureVerifier>,
}

impl KgConfig {
    pub fn new(consumer_schema: impl Into<String>, manifest_schema: impl Into<String>) -> Self {
        Self {
            consumer_schema: consumer_schema.into(),
            manifest_schema: manifest_schema.into(),
            mmap_bytes: DEFAULT_MMAP_BYTES,
            cache_kib: DEFAULT_CACHE_KIB,
            signature: Arc::new(UncheckedSignature),
        }
    }
}

impl fmt::Debug for KgConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KgConfig")
            .field("consumer_schema", &self.consumer_schema)
            .field("manifest_schema", &self.manifest_schema)
            .field("mmap_bytes", &self.mmap_bytes)
            .field("cache_kib", &self.cache_kib)
            .finish_non_exhaustive()
    }
}
