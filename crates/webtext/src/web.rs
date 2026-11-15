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
pub fn find_all(body: &str, query: &str) -> String {
    serde_json::to_string(&crate::find_all(body, query)).unwrap_or_default()
}

#[wasm_bindgen]
pub fn mark_layers(body: &str, cites: &str, finds: &str, current: i32) -> String {
    let cites: Vec<(usize, usize)> = serde_json::from_str(cites).unwrap_or_default();
    let finds: Vec<(usize, usize)> = serde_json::from_str(finds).unwrap_or_default();
    serde_json::to_string(&crate::mark_layers(body, &cites, &finds, usize::try_from(current).ok())).unwrap_or_default()
}

#[wasm_bindgen]
pub fn plain_math(text: &str) -> String {
    crate::plain_math(text)
}
