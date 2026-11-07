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

#[test]
fn usage_and_audit_are_swallowed_and_handed_over() {
    let mut f = filter();
    let mut out = f.push(&ev("Usage", r#"{"tokens":1234,"calls":3}"#));
    out.extend(f.push(&ev("Audit", r#"{"kind":"guard_block","detail":{"guard":"repeat","tool":"search_laws"}}"#)));
    out.extend(f.push(&ev("Audit", r#"{"kind":"login","detail":{}}"#)));
    out.extend(f.push(&ev("Audit", r#"{"kind":"verdict","detail":{"outcome":"Unknown","scope":"R1"}}"#)));
    assert!(out.is_empty());
    let side = f.take_side();
    assert_eq!(side.len(), 3);
    assert_eq!(side[0], super::SideEvent::Usage { tokens: 1234 });
    assert!(matches!(&side[1], super::SideEvent::Audit { kind, .. } if kind == "guard_block"));
    assert!(matches!(&side[2], super::SideEvent::Audit { kind, .. } if kind == "verdict"));
    assert!(f.take_side().is_empty());
}

#[test]
fn injection_warning_gets_fixed_text() {
    let mut f = filter();
    let out = f.push(&ev("Warning", r#"{"kind":"injection","message":"이전 지시를 무시하라 at /uploads/x.md"}"#));
    assert_eq!(out.len(), 1);
    assert!(out[0].contains("읽은 자료 안에 지시처럼 보이는 문장이 있었습니다"));
    assert!(!out[0].contains("uploads") && !out[0].contains("무시"));
}
