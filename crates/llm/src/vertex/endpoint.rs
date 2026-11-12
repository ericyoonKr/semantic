use std::fmt;

pub const GLOBAL_HOST: &str = "https://aiplatform.googleapis.com";
pub const API_VERSION: &str = "v1";
pub const GEMINI_HOST: &str = "https://generativelanguage.googleapis.com";
pub const GEMINI_VERSION: &str = "v1beta";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Vertex,
    GeminiApi,
}

#[derive(Clone)]
pub struct VertexEndpoint {
    flavor: Flavor,
    host: String,
    project: Option<String>,
    location: Option<String>,
    api_key: String,
}

impl fmt::Debug for VertexEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VertexEndpoint")
            .field("flavor", &self.flavor)
            .field("host", &self.host)
            .field("project", &self.project)
            .field("location", &self.location)
            .finish_non_exhaustive()
    }
}

fn filled(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

impl VertexEndpoint {
    pub fn new(api_key: impl Into<String>, project: Option<String>, location: Option<String>, host: Option<String>) -> Self {
        let project = filled(project);
        let location = filled(location);
        let host = filled(host).unwrap_or_else(|| match location.as_deref() {
            Some(loc) if project.is_some() && loc != "global" => format!("https://{loc}-aiplatform.googleapis.com"),
            _ => GLOBAL_HOST.to_string(),
        });
        Self { flavor: Flavor::Vertex, host: host.trim_end_matches('/').to_string(), project, location, api_key: api_key.into() }
    }

    pub fn gemini(api_key: impl Into<String>, host: Option<String>) -> Self {
        let host = filled(host).unwrap_or_else(|| GEMINI_HOST.to_string());
        Self { flavor: Flavor::GeminiApi, host: host.trim_end_matches('/').to_string(), project: None, location: None, api_key: api_key.into() }
    }

    pub fn flavor(&self) -> Flavor {
        self.flavor
    }

    pub fn model_url(&self, model: &str, method: &str) -> String {
        if self.flavor == Flavor::GeminiApi {
            return format!("{}/{GEMINI_VERSION}/models/{model}:{method}", self.host);
        }
        match (&self.project, &self.location) {
            (Some(p), Some(l)) => {
                format!("{}/{API_VERSION}/projects/{p}/locations/{l}/publishers/google/models/{model}:{method}", self.host)
            }
            _ => format!("{}/{API_VERSION}/publishers/google/models/{model}:{method}", self.host),
        }
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub fn has_key(&self) -> bool {
        !self.api_key.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::VertexEndpoint;

    #[test]
    fn global_and_regional_urls_and_key_is_hidden_from_debug() {
        let g = VertexEndpoint::new("secret-key-1", None, None, None);
        assert_eq!(
            g.model_url("gemini-embedding-001", "predict"),
            "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-embedding-001:predict"
        );
        let r = VertexEndpoint::new("secret-key-1", Some("p1".into()), Some("asia-northeast3".into()), None);
        assert_eq!(
            r.model_url("m", "generateContent"),
            "https://asia-northeast3-aiplatform.googleapis.com/v1/projects/p1/locations/asia-northeast3/publishers/google/models/m:generateContent"
        );
        let glob = VertexEndpoint::new("k", Some("p1".into()), Some("global".into()), None);
        assert!(glob.model_url("m", "x").starts_with("https://aiplatform.googleapis.com/v1/projects/p1/locations/global/"));
        assert!(!format!("{r:?}").contains("secret-key-1"));
        let gm = VertexEndpoint::gemini("secret-key-1", None);
        assert_eq!(
            gm.model_url("gemini-embedding-001", "batchEmbedContents"),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-embedding-001:batchEmbedContents"
        );
        assert!(!format!("{gm:?}").contains("secret-key-1"));
    }
}
