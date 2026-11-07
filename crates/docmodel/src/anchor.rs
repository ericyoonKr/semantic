use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub source: FileId,
    pub page: Option<u32>,
    pub bbox: Option<Rect>,
    pub path: Option<String>,
}
