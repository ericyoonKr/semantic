#[derive(Debug, thiserror::Error)]
pub enum KgError {
    #[error("file access failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("manifest is not valid")]
    Manifest,
    #[error("manifest format name does not match the configured value")]
    ManifestSchema,
    #[error("graph name or file name is not allowed")]
    BadName,
    #[error("manifest signature was rejected")]
    Signature,
    #[error("line {0}: not valid json")]
    Json(u64),
    #[error("first line is not a head")]
    HeadMissing,
    #[error("format name does not match the configured value")]
    Schema,
    #[error("graph shape is not registered")]
    Shape,
    #[error("head has no valid {0}")]
    HeadField(&'static str),
    #[error("line {0}: unknown item")]
    UnknownItem(u64),
    #[error("line {0}: item has no valid {1}")]
    ItemField(u64, &'static str),
    #[error("line {0}: text reference is not valid")]
    TextRef(u64),
    #[error("{kind} count {actual} does not match head count {expected}")]
    Count {
        kind: &'static str,
        expected: u64,
        actual: u64,
    },
    #[error("file sha256 does not match the manifest")]
    Sha256,
    #[error("line {0}: relation is outside the contract")]
    Relation(u64),
    #[error("a relation layer that must be acyclic has a cycle")]
    Cycle,
    #[error("graph name appears in more than one bundle")]
    DuplicateGraph,
    #[error("같은 갈래의 새 판이 있음")]
    Superseded,
    #[error("manifest source_bundle does not match the folder name")]
    SourceBundle,
    #[error("stored graph metadata is not valid")]
    Meta,
    #[error("unknown graph")]
    UnknownGraph,
    #[error("unknown node")]
    UnknownNode,
    #[error("relation is not allowed by the contract")]
    RelationNotAllowed,
    #[error("invalid argument: {0}")]
    Argument(&'static str),
    #[error("connection pool is unavailable")]
    Pool,
}
