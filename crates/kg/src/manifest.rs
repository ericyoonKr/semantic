use serde::Deserialize;
use serde_json::Value;

use crate::error::KgError;

#[derive(Debug, Deserialize)]
pub(crate) struct Manifest {
    pub schema: String,
    #[serde(default)]
    pub source_bundle: Option<String>,
    pub graphs: Vec<GraphEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphEntry {
    #[serde(alias = "graph")]
    pub name: String,
    pub file: String,
    pub sha256: String,
    #[serde(flatten)]
    pub rest: serde_json::Map<String, Value>,
}

impl Manifest {
    pub(crate) fn parse(bytes: &[u8], expected_schema: &str) -> Result<Self, KgError> {
        let manifest: Manifest = serde_json::from_slice(bytes).map_err(|_| KgError::Manifest)?;
        if manifest.schema != expected_schema {
            return Err(KgError::ManifestSchema);
        }
        for entry in &manifest.graphs {
            if !plain_name(&entry.name) || !plain_name(&entry.file) {
                return Err(KgError::BadName);
            }
        }
        Ok(manifest)
    }
}

impl GraphEntry {
    pub(crate) fn as_value(&self) -> Value {
        let mut map = self.rest.clone();
        map.insert("name".into(), Value::String(self.name.clone()));
        map.insert("file".into(), Value::String(self.file.clone()));
        map.insert("sha256".into(), Value::String(self.sha256.clone()));
        Value::Object(map)
    }
}

pub(crate) fn plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}
