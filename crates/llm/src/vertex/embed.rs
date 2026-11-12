use serde_json::{Value, json};

use super::endpoint::{Flavor, VertexEndpoint};
use super::error::{self, VertexError};

pub const QUERY: &str = "RETRIEVAL_QUERY";
pub const DOCUMENT: &str = "RETRIEVAL_DOCUMENT";
pub const MAX_INPUT_TOKENS: u32 = 2048;
pub const GEMINI_BATCH_MAX: usize = 100;

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

pub fn gemini_request(model: &str, req: &EmbedRequest<'_>) -> Value {
    let mut r = json!({
        "model": format!("models/{model}"),
        "content": { "parts": [{ "text": req.text }] },
        "taskType": req.task,
        "outputDimensionality": req.dimensions,
    });
    if let Some(t) = req.title.filter(|t| req.task == DOCUMENT && !t.trim().is_empty()) {
        r["title"] = json!(t);
    }
    r
}

fn values_of(v: &Value, dimensions: usize) -> Result<Vec<f32>, VertexError> {
    let mut values: Vec<f32> = v
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
    Ok(values)
}

pub fn read_gemini_batch(payload: &Value, count: usize, dimensions: usize) -> Result<Vec<EmbedReply>, VertexError> {
    let items = payload
        .get("embeddings")
        .and_then(Value::as_array)
        .ok_or_else(|| VertexError::Malformed("no embeddings".into()))?;
    if items.len() != count {
        return Err(VertexError::Malformed(format!("asked {count} embeddings, got {}", items.len())));
    }
    items.iter().map(|e| Ok(EmbedReply { values: values_of(e, dimensions)?, tokens: 0, truncated: false })).collect()
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
    if let Some(e) = payload.get("embedding") {
        return Ok(EmbedReply { values: values_of(e, dimensions)?, tokens: 0, truncated: false });
    }
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

async fn post(http: &reqwest::Client, endpoint: &VertexEndpoint, url: String, body: &Value) -> Result<Value, VertexError> {
    let response = http
        .post(url)
        .header("x-goog-api-key", endpoint.api_key())
        .json(body)
        .send()
        .await
        .map_err(|e| error::from_send(&e))?;
    let status = response.status();
    let hint = error::retry_after_header(response.headers());
    let text = response.text().await.map_err(|e| error::from_send(&e))?;
    if !status.is_success() {
        return Err(error::from_status(status.as_u16(), &text, hint));
    }
    serde_json::from_str(&text).map_err(|e| VertexError::Malformed(e.to_string()))
}

pub async fn embed_once(
    http: &reqwest::Client,
    endpoint: &VertexEndpoint,
    model: &str,
    req: &EmbedRequest<'_>,
) -> Result<EmbedReply, VertexError> {
    let (method, payload) = match endpoint.flavor() {
        Flavor::Vertex => ("predict", body(req)),
        Flavor::GeminiApi => ("embedContent", gemini_request(model, req)),
    };
    let reply = post(http, endpoint, endpoint.model_url(model, method), &payload).await?;
    read(&reply, req.dimensions)
}

pub async fn embed_batch_once(
    http: &reqwest::Client,
    endpoint: &VertexEndpoint,
    model: &str,
    reqs: &[EmbedRequest<'_>],
) -> Result<Vec<EmbedReply>, VertexError> {
    if endpoint.flavor() != Flavor::GeminiApi || reqs.len() > GEMINI_BATCH_MAX {
        return Err(VertexError::BadRequest {
            code: 400,
            status: "LOCAL".into(),
            message: format!("batch needs the Gemini API and at most {GEMINI_BATCH_MAX} texts, got {}", reqs.len()),
        });
    }
    let Some(dimensions) = reqs.first().map(|r| r.dimensions) else {
        return Ok(Vec::new());
    };
    let payload = json!({ "requests": reqs.iter().map(|r| gemini_request(model, r)).collect::<Vec<_>>() });
    let reply = post(http, endpoint, endpoint.model_url(model, "batchEmbedContents"), &payload).await?;
    read_gemini_batch(&reply, reqs.len(), dimensions)
}

#[cfg(test)]
mod tests {
    use super::{DOCUMENT, EmbedRequest, QUERY, body, gemini_request, read, read_gemini_batch};
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

    #[test]
    fn gemini_api_shapes_are_built_read_and_normalized() {
        let r = gemini_request("gemini-embedding-001", &EmbedRequest { text: "t", task: DOCUMENT, title: Some("법"), dimensions: 1024 });
        assert_eq!(r["model"], "models/gemini-embedding-001");
        assert_eq!(r["content"]["parts"][0]["text"], "t");
        assert_eq!(r["taskType"], DOCUMENT);
        assert_eq!(r["title"], "법");
        assert_eq!(r["outputDimensionality"], 1024);
        let one = read(&json!({"embedding":{"values":[3.0,4.0]}}), 2).map(|x| x.values).map_err(|e| e.to_string());
        assert_eq!(one, Ok(vec![0.6, 0.8]));
        let batch = json!({"embeddings":[{"values":[0.0,2.0]},{"values":[1.0,0.0]}]});
        let got = read_gemini_batch(&batch, 2, 2).map(|v| v.into_iter().map(|x| x.values).collect::<Vec<_>>()).map_err(|e| e.to_string());
        assert_eq!(got, Ok(vec![vec![0.0, 1.0], vec![1.0, 0.0]]));
        assert!(read_gemini_batch(&batch, 3, 2).is_err());
    }
}
