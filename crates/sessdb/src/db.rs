use std::path::Path;
use std::sync::Arc;

use rocksdb::{BlockBasedOptions, Cache, ColumnFamily, ColumnFamilyDescriptor, DB, Options};

use crate::error::StorageError;

pub(crate) const CF_STATE: &str = "state";
pub(crate) const CF_LEAF: &str = "leaf_cache";

const MIB: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tuning {
    pub block_cache_bytes: usize,
    pub write_buffer_bytes: usize,
    pub max_write_buffers: i32,
    pub background_jobs: i32,
}

impl Default for Tuning {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
        let jobs = i32::try_from(cores / 2).unwrap_or(1).max(1);
        Self {
            block_cache_bytes: 256 * MIB,
            write_buffer_bytes: 64 * MIB,
            max_write_buffers: 2,
            background_jobs: jobs,
        }
    }
}

#[derive(Clone)]
pub struct SessDb(pub(crate) Arc<DB>);

impl std::fmt::Debug for SessDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessDb").finish_non_exhaustive()
    }
}

pub fn open(path: &Path) -> Result<SessDb, StorageError> {
    open_with(path, &Tuning::default())
}

pub fn open_with(path: &Path, tuning: &Tuning) -> Result<SessDb, StorageError> {
    let cache = Cache::new_lru_cache(tuning.block_cache_bytes);
    let mut table = BlockBasedOptions::default();
    table.set_block_cache(&cache);
    let cf_opts = || {
        let mut o = Options::default();
        o.set_block_based_table_factory(&table);
        o.set_write_buffer_size(tuning.write_buffer_bytes);
        o.set_max_write_buffer_number(tuning.max_write_buffers);
        o
    };
    let mut db_opts = cf_opts();
    db_opts.create_if_missing(true);
    db_opts.create_missing_column_families(true);
    db_opts.set_max_background_jobs(tuning.background_jobs);
    let cfs = [CF_STATE, CF_LEAF].map(|name| ColumnFamilyDescriptor::new(name, cf_opts()));
    let db = DB::open_cf_descriptors(&db_opts, path, cfs)?;
    Ok(SessDb(Arc::new(db)))
}

impl SessDb {
    pub(crate) fn cf(&self, name: &'static str) -> Result<&ColumnFamily, StorageError> {
        self.0
            .cf_handle(name)
            .ok_or(StorageError::MissingColumnFamily(name))
    }

    pub fn flush(&self) -> Result<(), StorageError> {
        for name in [CF_STATE, CF_LEAF] {
            self.0.flush_cf(self.cf(name)?)?;
        }
        self.0.flush_wal(true)?;
        Ok(())
    }
}
