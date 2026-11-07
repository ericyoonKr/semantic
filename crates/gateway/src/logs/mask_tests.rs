use super::{MASK, mask};

const MUST_MASK: &[&str] = &[
    "900101-1234567",
    "851231-2345678",
    "020315-3456789",
    "000101-4123456",
    "9001011234567",
    "8512312345678",
    "010-1234-5678",
    "010-123-4567",
    "011-234-5678",
    "016-9876-5432",
    "01012345678",
    "0101234567",
    "02-123-4567",
    "02-1234-5678",
    "031-123-4567",
    "051-1234-5678",
    "064-712-3456",
    "0212345678",
    "0311234567",
    "070-1234-5678",
    "110-123-456789",
    "1002-123-456789",
    "123-456789-01-001",
    "356-0123-4567-83",
    "301-0123-4567-01",
    "79-123456-78",
    "123-12-123456",
    "352-1234-5678-93",
    "100-012-345678",
    "1234567890123",
    "12345678901",
    "3333-01-1234567",
    "94-1234567890",
];

const MUST_KEEP: &[&str] = &[
    "2026-11-08",
    "2023-01-05",
    "12:34:56",
    "2026-11-08T12:34:56.123456Z",
    "2022두1234",
    "2019다12345",
    "2020헌마123",
    "550e8400-e29b-41d4-a716-446655440000",
    "123e4567-e89b-12d3-a456-426614174000",
    "9f86d081884c7d659a2feaa0c55ad015",
    "a3b1c2d4e5f60718293a4b5c6d7e8f90",
    "localhost:8090",
    "port=8000",
    "8080",
    "209715200",
    "size=104857600",
    "elapsed_ms=1234",
    "took 3.5s",
    "제750조",
    "제23조의2",
    "제1항",
    "429",
    "status=503",
    "available_mib=987",
    "v0.1.0",
    "2026. 11. 8.",
    "gateway-2026-11-08.1.log",
    "rid-1",
    "auth_test_0a1b2c3d4e5f",
    "123456789",
    "20261108123456",
    "1-2-3-4-5",
    "9b2c4d1e-7f3a-4c2b-9d8e-310045678912",
    "12345678-1234-1234-1234-123456789012",
    "key-01012345678",
];

const OUTSIDE_RULE: &[&str] = &["010 1234 5678", "010.1234.5678"];

fn masked(token: &str) -> bool {
    let line = format!("event x={token} y");
    mask(&line).contains(MASK)
}

#[test]
fn masking_accuracy_on_built_sample() {
    let misses: Vec<&str> = MUST_MASK.iter().copied().filter(|t| !masked(t)).collect();
    let false_masks: Vec<&str> = MUST_KEEP.iter().copied().filter(|t| masked(t)).collect();
    let outside: Vec<&str> = OUTSIDE_RULE.iter().copied().filter(|t| !masked(t)).collect();
    println!(
        "must_mask={} misses={} {:?}; must_keep={} false_masks={} {:?}; outside_rule={} unmasked={}",
        MUST_MASK.len(),
        misses.len(),
        misses,
        MUST_KEEP.len(),
        false_masks.len(),
        false_masks,
        OUTSIDE_RULE.len(),
        outside.len()
    );
    assert!(misses.is_empty());
    assert_eq!(false_masks, vec!["20261108123456"]);
}

#[test]
fn surrounding_text_is_kept() {
    assert_eq!(mask("전화 010-1234-5678 로"), format!("전화 {MASK} 로"));
    assert_eq!(mask("id=900101-1234567, ok"), format!("id={MASK}, ok"));
    assert_eq!(mask("a-01012345678-b"), "a-01012345678-b");
    assert_eq!(mask("no digits here"), "no digits here");
}
