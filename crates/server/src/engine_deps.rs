// engine 트레이트 구현 — engine::deps의 FactSearch·AuthoritySearch·LeafCache (PROJECT.md §7)
// engine은 DB·세션 저장소·검색을 모른다. 여기서 실제 구현(retrieval·sessdb·moka)을 붙인다.
//
// 담을 것:
// - struct UploadFacts { embed: EmbedClient, sessdb: SessDb }   impl FactSearch
//     candidates(session, proposition, k): proposition 임베딩 → sessdb state의 UploadIndex 복원 →
//     top_k → 조항 원문·anchor를 붙여 FactCandidate로
// - struct CorpusAuthority { pools, rewriter 캐시, embed }        impl AuthoritySearch
//     search(query, k): retrieval::search → SearchHit를 engine::deps::AuthorityHit로 옮긴다.
//     snapshot_hash = 결과 출처 id들의 sha2 (cache_key 재료), degraded는 그대로 전달.
// - struct LeafStore { mem: moka::future::Cache<String, LeafResult>, sessdb: SessDb }  impl LeafCache
//     get: moka → 없으면 sessdb get_leaf(바이트) → serde_json으로 LeafResult 복원 → moka에 넣음
//     put: serde_json 바이트로 sessdb put_leaf + moka 갱신
//     같은 키 동시 미스는 moka get_with로 하나로 합친다 (§10 캐시 스탬피드).
//
// 규칙:
// - 실패는 engine::deps::DepError로 돌려준다 — 판정 불가로 바꾸는 일은 engine이 한다.
// - 긴 sessdb 순회만 spawn_blocking (§10).
