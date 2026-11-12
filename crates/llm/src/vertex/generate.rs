use serde_json::{Value, json};

use super::endpoint::VertexEndpoint;
use super::error::{self, VertexError};

const BLOCK_REASONS: [&str; 6] = ["SAFETY", "RECITATION", "BLOCKLIST", "PROHIBITED_CONTENT", "SPII", "IMAGE_SAFETY"];

#[derive(Debug, Clone)]
pub struct GenerateRequest<'a> {
    pub system: &'a str,
    pub user: &'a str,
    pub max_tokens: u32,
    pub json_mode: bool,
    pub thinking_budget: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenerateReply {
    pub text: String,
    pub prompt_tokens: u64,
    pub output_tokens: u64,
    pub thinking_tokens: u64,
}

pub fn body(req: &GenerateRequest<'_>) -> Value {
    let mut config = json!({ "temperature": 0, "maxOutputTokens": req.max_tokens, "candidateCount": 1 });
    if req.json_mode {
        config["responseMimeType"] = json!("application/json");
    }
    if let Some(b) = req.thinking_budget {
        config["thinkingConfig"] = json!({ "thinkingBudget": b });
    }
    json!({
        "systemInstruction": { "parts": [{ "text": req.system }] },
        "contents": [{ "role": "user", "parts": [{ "text": req.user }] }],
        "generationConfig": config,
    })
}

fn count(v: &Value, key: &str) -> u64 {
    v.get("usageMetadata").and_then(|u| u.get(key)).and_then(Value::as_u64).unwrap_or(0)
}

pub fn read(payload: &Value) -> Result<GenerateReply, VertexError> {
    if let Some(reason) = payload.get("promptFeedback").and_then(|p| p.get("blockReason")).and_then(Value::as_str) {
        return Err(VertexError::Blocked(format!("prompt {reason}")));
    }
    let cand = payload
        .get("candidates")
        .and_then(|c| c.get(0))
        .ok_or_else(|| VertexError::Malformed("no candidates".into()))?;
    let finish = cand.get("finishReason").and_then(Value::as_str).unwrap_or("");
    if BLOCK_REASONS.contains(&finish) {
        return Err(VertexError::Blocked(finish.to_string()));
    }
    if finish == "MAX_TOKENS" {
        return Err(VertexError::Truncated);
    }
    let text: String = cand
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter(|p| !p.get("thought").and_then(Value::as_bool).unwrap_or(false))
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    if text.trim().is_empty() {
        return Err(VertexError::Malformed(format!("empty text (finish {finish:?})")));
    }
    Ok(GenerateReply {
        text,
        prompt_tokens: count(payload, "promptTokenCount"),
        output_tokens: count(payload, "candidatesTokenCount"),
        thinking_tokens: count(payload, "thoughtsTokenCount"),
    })
}

pub async fn generate_once(
    http: &reqwest::Client,
    endpoint: &VertexEndpoint,
    model: &str,
    req: &GenerateRequest<'_>,
) -> Result<GenerateReply, VertexError> {
    let response = http
        .post(endpoint.model_url(model, "generateContent"))
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
    let reply = read(&payload)?;
    tracing::info!(
        model,
        prompt_tokens = reply.prompt_tokens,
        output_tokens = reply.output_tokens,
        thinking_tokens = reply.thinking_tokens,
        "vertex.generate"
    );
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::{GenerateRequest, body, read};
    use crate::vertex::error::VertexError;
    use serde_json::json;

    #[test]
    fn body_carries_system_user_and_json_mode() {
        let b = body(&GenerateRequest { system: "s", user: "u", max_tokens: 64, json_mode: true, thinking_budget: Some(0) });
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "s");
        assert_eq!(b["contents"][0]["parts"][0]["text"], "u");
        assert_eq!(b["generationConfig"]["responseMimeType"], "application/json");
        assert_eq!(b["generationConfig"]["thinkingConfig"]["thinkingBudget"], 0);
    }

    #[test]
    fn reply_skips_thoughts_and_flags_blocks_and_truncation() {
        let ok = json!({"candidates":[{"content":{"parts":[{"text":"생각","thought":true},{"text":"{\"a\":1}"}]},"finishReason":"STOP"}],
            "usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":3,"thoughtsTokenCount":5}});
        let r = read(&ok).map_err(|e| e.to_string());
        assert_eq!(r.as_ref().map(|x| x.text.as_str()), Ok("{\"a\":1}"));
        assert_eq!(r.map(|x| (x.prompt_tokens, x.thinking_tokens)), Ok((10, 5)));
        assert!(matches!(read(&json!({"candidates":[{"finishReason":"SAFETY"}]})), Err(VertexError::Blocked(_))));
        assert!(matches!(read(&json!({"promptFeedback":{"blockReason":"OTHER"}})), Err(VertexError::Blocked(_))));
        assert!(matches!(read(&json!({"candidates":[{"content":{"parts":[{"text":"x"}]},"finishReason":"MAX_TOKENS"}]})), Err(VertexError::Truncated)));
    }
}
