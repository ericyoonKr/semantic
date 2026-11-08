use serde_json::Value;

use super::{Item, ShapeReader};
use crate::error::KgError;

pub(crate) struct Plain;

impl ShapeReader for Plain {
    fn read(&self, tag: &str, body: Value, line_no: u64) -> Result<Vec<Item>, KgError> {
        match tag {
            "node" => Ok(vec![Item::Node(body)]),
            "edge" => Ok(vec![Item::Edge(body)]),
            "unattached_state" => Ok(vec![Item::State(body)]),
            _ => Err(KgError::UnknownItem(line_no)),
        }
    }
}
