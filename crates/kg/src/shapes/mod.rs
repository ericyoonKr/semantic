mod plain;

use serde_json::Value;

use crate::error::KgError;

pub(crate) enum Item {
    Node(Value),
    Edge(Value),
    State(Value),
}

pub(crate) trait ShapeReader: Sync {
    fn read(&self, tag: &str, body: Value, line_no: u64) -> Result<Vec<Item>, KgError>;
}

static REGISTRY: &[(&str, &dyn ShapeReader)] = &[
    ("연결망", &plain::Plain),
    ("항목 목록", &plain::Plain),
    ("찾아보기", &plain::Plain),
    ("나무", &plain::Plain),
];

pub(crate) fn reader(shape: &str) -> Option<&'static dyn ShapeReader> {
    REGISTRY
        .iter()
        .find(|(name, _)| *name == shape)
        .map(|(_, r)| *r)
}
