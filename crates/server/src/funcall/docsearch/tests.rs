use super::{Passage, decode_vectors, encode_vectors, fuse, keyword_scores, passages, query_words, vector_scores};

#[test]
fn text_is_cut_into_passages_that_keep_their_page() {
    let text = format!("--- p1 ---\n머리말\n--- p2 ---\n{}\n짧은 끝\n", "가".repeat(1300));
    let got = passages(&text);
    assert_eq!(got[0], Passage { page: Some(1), text: "머리말".into() });
    assert!(got[1..].iter().all(|p| p.page == Some(2)));
    assert!(got.iter().all(|p| p.text.chars().count() <= 610));
    assert_eq!(got.last().map(|p| p.text.ends_with("짧은 끝")), Some(true));
}

#[test]
fn literal_words_and_shared_pieces_rank_passages() {
    let ps = vec![
        Passage { page: Some(1), text: "요건사실의 의의와 증명책임".into() },
        Passage { page: Some(2), text: "소멸시효 항변의 요건사실은 권리를 행사할 수 있는 때부터 기간이 지났다는 사실이다".into() },
        Passage { page: Some(3), text: "전혀 다른 이야기".into() },
    ];
    let ranked = keyword_scores("소멸시효 항변의 요건사실은?", &ps);
    assert_eq!(ranked.first().map(|r| r.0), Some(1));
    assert!(ranked.iter().all(|r| r.0 != 2));
    assert_eq!(query_words("소멸시효 항변의 요건사실은?"), vec!["소멸시효", "항변", "요건사실"]);
}

#[test]
fn keyword_and_vector_lists_are_fused_and_vectors_round_trip() {
    let vectors = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![0.7, 0.7]];
    let by_vector = vector_scores(&[0.0, 1.0], &vectors);
    assert_eq!(by_vector[0].0, 1);
    let fused = fuse(&[vec![(2, 5.0), (0, 1.0)], by_vector], 10);
    assert_eq!(fused[0].0, 2);
    let bytes = encode_vectors(&vectors);
    assert_eq!(decode_vectors(&bytes, 3), Some(vectors));
    assert_eq!(decode_vectors(&bytes, 4), None);
}
