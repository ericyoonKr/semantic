use super::SseFilter;

fn ev(name: &str, data: &str) -> String {
    format!("event: {name}\ndata: {data}\n\n")
}

fn filter() -> SseFilter {
    SseFilter::new("rid-1", vec![("<think>".into(), "</think>".into())])
}

#[test]
fn tool_names_become_short_status_once() {
    let mut f = filter();
    let mut out = f.push(&ev("Status", r#"{"phase":"start","tool":"search_cases"}"#));
    out.extend(f.push(&ev("Status", r#"{"phase":"start","tool":"read_case"}"#)));
    out.extend(f.push(&ev("Status", r#"{"phase":"start","tool":"judge"}"#)));
    let joined = out.join("");
    assert_eq!(out.len(), 2);
    assert!(joined.contains("자료를 찾는 중") && joined.contains("판단하는 중"));
    assert!(!joined.contains("search_cases") && !joined.contains("read_case") && !joined.contains("judge"));
}

#[test]
fn split_frames_are_reassembled() {
    let mut f = filter();
    let whole = ev("Token", r#"{"text":"안녕하세요"}"#);
    let (a, b) = whole.split_at(10);
    assert!(f.push(a).is_empty());
    let out = f.push(b);
    assert_eq!(out.len(), 1);
    assert!(out[0].contains("안녕하세요"));
}

#[test]
fn thinking_span_is_hidden_across_chunks() {
    let mut f = filter();
    let mut out = f.push(&ev("Token", r#"{"text":"앞 <think>속생각"}"#));
    out.extend(f.push(&ev("Token", r#"{"text":"계속</think>뒤"}"#)));
    let joined = out.join("");
    assert!(joined.contains("앞 ") && joined.contains("뒤"));
    assert!(!joined.contains("속생각") && !joined.contains("계속"));
}

#[test]
fn unknown_events_and_fields_are_dropped() {
    let mut f = filter();
    assert!(f.push(&ev("Debug", r#"{"x":1}"#)).is_empty());
    let out = f.push(&ev("VerdictReady", r#"{"text":"판정","node_ids":["R1"],"cache_key":"k"}"#));
    assert_eq!(out.len(), 1);
    assert!(!out[0].contains("R1") && !out[0].contains("cache_key"));
    assert!(f.push(&ev("Warning", r#"{"kind":"model_server","message":"vllm down"}"#)).is_empty());
}

#[test]
fn errors_are_generic_with_request_id() {
    let mut f = filter();
    let out = f.push(&ev("Error", r#"{"message":"Traceback at C:\\path\\x.py"}"#));
    assert_eq!(out.len(), 1);
    assert!(out[0].contains("rid-1"));
    assert!(!out[0].contains("Traceback") && !out[0].contains("path"));
}
