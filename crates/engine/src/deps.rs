// engine이 바깥에서 받는 기능 — 트레이트로 정하고 server가 구현한다 (의존성 역전, PROJECT.md §7)
//
// 왜: engine은 판정 규칙을 담는 안쪽 층이다. DB(sqlx)·세션 저장소(sessdb)·검색(retrieval)을
//     직접 알면 바깥 구현이 바뀔 때마다 engine이 흔들리고, 단위 테스트에 DB·서버가 필요해진다.
//     engine은 「무엇이 필요한지」만 트레이트로 적고, 구현은 server/src/engine_deps.rs가 한다.
//     테스트는 가짜 구현(고정된 조항·판례를 돌려주는 구조체)으로 돌린다.
//
// 담을 것:
// - trait FactSearch     { async fn candidates(&self, session: &str, proposition: &str, k: usize)
//                              -> Result<Vec<FactCandidate>, DepError>; }
//     리프 문장과 가까운 업로드 문서 조항 (server 구현: 임베딩 → retrieval::upload_index::top_k)
// - trait AuthoritySearch { async fn search(&self, query: &str, k: usize)
//                              -> Result<AuthoritySnapshot, DepError>; }
//     법령·판례 근거 (server 구현: retrieval::search). snapshot_hash는 cache_key 재료.
// - trait LeafCache      { async fn get(&self, key: &str) -> Option<LeafResult>;
//                          async fn put(&self, key: &str, v: &LeafResult) -> Result<(), DepError>; }
//     (server 구현: moka 앞단 + sessdb leaf_cache CF — 동시 미스 합류는 moka get_with, §10)
// - struct FactCandidate { clause_id: ClauseId, text: String, anchor: Anchor }   // anchor 필수
// - struct AuthorityHit { source: String, text: String, 출처 필드(법령명·조문번호·시행일 /
//                         사건번호·선고일) }  — retrieval::SearchHit를 server가 이 타입으로 옮긴다
// - struct AuthoritySnapshot { hits: Vec<AuthorityHit>, snapshot_hash: String, degraded: bool }
// - enum DepError { Unavailable(설명), Other(설명) }  → leaf.rs가 FailureReason으로 바꿔 Unknown
//
// 규칙:
// - async fn in trait은 dyn으로 쓸 수 없다 — EngineDeps<F, A, C>처럼 제네릭으로 받는다.
// - 실패는 전부 DepError로 돌려주고, engine은 그것을 Unknown + 사유로만 바꾼다 (§2 원칙 2).
