use serde_json::{Value, json};

use super::endpoint::VertexEndpoint;
use super::error::{self, VertexError};

pub const QUERY: &str = "RETRIEVAL_QUERY";
pub const DOCUMENT: &str = "RETRIEVAL_DOCUMENT";
pub const MAX_INPUT_TOKENS: u32 = 2048;

#[derive(Debug, Clone)]
pub struct EmbedRequest<'a> {
    pub text: &'a str,
    pub task: &'a str,
    pub title: Option<&'a str>,
    pub dimensions: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbedReply {
    pub values: Vec<f32>,
    pub tokens: u64,
    pub truncated: bool,
}

pub fn body(req: &EmbedRequest<'_>) -> Value {
    let mut instance = json!({ "content": req.text, "task_type": req.task });
    if let Some(t) = req.title.filter(|t| req.task == DOCUMENT && !t.trim().is_empty()) {
        instance["title"] = json!(t);
    }
    json!({ "instances": [instance], "parameters": { "autoTruncate": true, "outputDimensionality": req.dimensions } })
}

pub fn normalize(v: &mut [f32]) -> Result<(), VertexError> {
    let norm = v.iter().map(|x| f64::from(*x) * f64::from(*x)).sum::<f64>().sqrt();
    if !norm.is_finite() || norm == 0.0 {
        return Err(VertexError::Malformed("zero or non-finite embedding".into()));
    }
    for x in v.iter_mut() {
        *x = (f64::from(*x) / norm) as f32;
    }
    Ok(())
}

pub fn read(payload: &Value, dimensions: usize) -> Result<EmbedReply, VertexError> {
    let emb = payload
        .get("predictions")
        .and_then(|p| p.get(0))
        .and_then(|p| p.get("embeddings"))
        .ok_or_else(|| VertexError::Malformed("no predictions".into()))?;
    let mut values: Vec<f32> = emb
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| VertexError::Malformed("no values".into()))?
        .iter()
        .map(|x| x.as_f64().map(|f| f as f32))
        .collect::<Option<_>>()
        .ok_or_else(|| VertexError::Malformed("non-numeric value".into()))?;
    if values.len() != dimensions {
        return Err(VertexError::Malformed(format!("expected {dimensions} values, got {}", values.len())));
    }
    normalize(&mut values)?;
    let stats = emb.get("statistics");
    Ok(EmbedReply {
        values,
        tokens: stats.and_then(|s| s.get("token_count")).and_then(Value::as_f64).map_or(0, |t| t as u64),
        truncated: stats.and_then(|s| s.get("truncated")).and_then(Value::as_bool).unwrap_or(false),
    })
}

pub async fn embed_once(
    http: &reqwest::Client,
    endpoint: &VertexEndpoint,
    model: &str,
    req: &EmbedRequest<'_>,
) -> Result<EmbedReply, VertexError> {
    let response = http
        .post(endpoint.model_url(model, "predict"))
        .header("x-goog-api-key", endpoint.api_key())
        .json(&body(req))
        .send()
        .await
        .map_err(|e| error::from_send(&e))?;
    let status = response.status();
    let hint = error::retry_after_header(response.headers());
    let text = response.text().await.map_err(|e| error::from_send(&e))?;
    if !status.is_success() {
        return Err(error::from_status(status.as_u16(), &text, hint));
    }
    let payload: Value = serde_json::from_str(&text).map_err(|e| VertexError::Malformed(e.to_string()))?;
    read(&payload, req.dimensions)
}

#[cfg(test)]
mod tests {
    use super::{DOCUMENT, EmbedRequest, QUERY, body, read};
    use crate::vertex::error::VertexError;
    use serde_json::json;

    #[test]
    fn body_sets_task_dimensions_and_title_only_for_documents() {
        let d = body(&EmbedRequest { text: "t", task: DOCUMENT, title: Some("행정절차법 제21조"), dimensions: 1024 });
        assert_eq!(d["instances"][0]["title"], "행정절차법 제21조");
        assert_eq!(d["parameters"]["outputDimensionality"], 1024);
        let q = body(&EmbedRequest { text: "t", task: QUERY, title: Some("x"), dimensions: 1024 });
        assert!(q["instances"][0].get("title").is_none());
        assert_eq!(q["instances"][0]["task_type"], QUERY);
    }

    #[test]
    fn reply_is_normalized_and_checked() {
        let p = json!({"predictions":[{"embeddings":{"values":[3.0,4.0],"statistics":{"token_count":7.0,"truncated":true}}}]});
        let r = read(&p, 2).map_err(|e| e.to_string());
        assert_eq!(r.as_ref().map(|x| x.values.clone()), Ok(vec![0.6, 0.8]));
        assert_eq!(r.map(|x| (x.tokens, x.truncated)), Ok((7, true)));
        assert!(matches!(read(&p, 3), Err(VertexError::Malformed(_))));
        let zero = json!({"predictions":[{"embeddings":{"values":[0.0,0.0]}}]});
        assert!(matches!(read(&zero, 2), Err(VertexError::Malformed(_))));
    }
}
