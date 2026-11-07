#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("rocksdb failed: {0}")]
    Rocks(#[from] rocksdb::Error),
    #[error("column family {0} is missing")]
    MissingColumnFamily(&'static str),
    #[error("key part {0} is not valid")]
    Key(&'static str),
}
