use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub fn split_body(body: &str) -> String {
    serde_json::to_string(&crate::split_body(body)).unwrap_or_default()
}

#[wasm_bindgen]
pub fn mark_spans(body: &str, spans: &str) -> String {
    let spans: Vec<(usize, usize)> = serde_json::from_str(spans).unwrap_or_default();
    serde_json::to_string(&crate::mark_spans(body, &spans)).unwrap_or_default()
}

#[wasm_bindgen]
pub fn plain_math(text: &str) -> String {
    crate::plain_math(text)
}
