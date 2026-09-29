// 리프별 근거 조립 — fact slice + authority slice (PROJECT.md §6.2, §7.1)
// leaf.rs(판정)와 나눈 이유: 근거를 어떻게 모으는지(검색 전략)와 LLM에 어떻게 묻는지(호출 규약)는
// 서로 다른 이유로 바뀐다.
//
// 담을 것:
// - struct LeafRequest { node_id, proposition, fact_slice: Vec<FactCandidate>,
//                        authority: AuthoritySnapshot, cache_key: String }
// - async fn build_leaf_requests<F: FactSearch, A: AuthoritySearch>(
//       subtree: &ClaimGraph, session: &str, facts: &F, authority: &A) -> Vec<Prepared>
//     enum Prepared { Ready(LeafRequest), NoEvidence(NodeId) }
//   로직:
//   1) 리프마다 facts.candidates ∥ authority.search (tokio::join! — §10 병렬)
//   2) fact slice가 비면 Prepared::NoEvidence — LLM을 부르지 않고
//      Unknown + FailureReason::NoEvidence("업로드 문서에서 근거 조항을 찾지 못함")로 간다.
//   3) cache_key = schema::cache_key(node_id, fact slice 해시, authority.snapshot_hash)
//   4) DepError는 그 리프만 Unknown + 사유 (다른 리프는 계속)
//
// 규칙: fact slice 항목은 FactCandidate라 anchor가 반드시 있다 — anchor 없는 사실이
//       LLM 판정의 근거로 들어갈 길이 없다 (anchor.rs 규칙).
