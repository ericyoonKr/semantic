// engine — ⑤ 법률 엔진: Leaf 구조 + Z3 (PROJECT.md §7 전체)
// 기준: 뉴로심볼릭 가이드북 (~/Downloads/백업본-뉴로심볼릭-20260913/) Rust 이식.
//
// 두 원칙 (§2):
// 1) LLM은 원자 명제 판정기 — 전체 결론("이 처분은 위법한가?")을 묻지 않는다.
// 2) 모든 실패는 Unknown/판정 불가 방향으로만 무너진다.
//
// 진입점 개요:
//   pub struct JudgeRequest { session: String, root: NodeId,
//                             assumptions: HashMap<NodeId, bool> }   // 가정 질문 §6.3
//   pub struct EngineDeps<F: FactSearch, A: AuthoritySearch, C: LeafCache> {
//       llm: LlmClient, limiter: LlmLimiter, facts: F, authority: A, cache: C }  // deps.rs
//   pub async fn judge<F, A, C>(graph: &ClaimGraph, req: JudgeRequest, deps: &EngineDeps<F, A, C>,
//                               cancel: CancellationToken) -> Result<Verdict, EngineError>
//   1) graph: 서브트리 추출 (어떤 노드든 루트 가능 — 부분 질문 §6.3)
//   2) slices: 리프별 근거 조립 → leaf: 리프 판정 (cache 조회 → LLM fan-out → 모순 검출)
//   3) compile: Z3 식 조립 (spawn_blocking, Context는 그 안에서 생성·폐기)
//   4) verdict: sat/unsat/unknown + unsat_core → Verdict
//
// 두 속도 (§6.2): 결정론 검증(checks)은 문서 버전마다 전체 재실행,
//   여기(느린 경로)는 cache_key 바뀐 리프만 재판정.
//
// 이 crate에서 async는 slices.rs·leaf.rs(바깥 호출·LLM 오케스트레이션)뿐.
// schema/graph/compile/verdict는 동기 — 단위 테스트와 정확도 실측이 런타임 없이 가능해야 한다 (§10).
// DB·세션 저장소·검색은 직접 부르지 않고 deps.rs의 트레이트로 받는다 (의존성 역전).

pub mod compile;
pub mod deps;
pub mod graph;
pub mod leaf;
pub mod schema;
pub mod slices;
pub mod verdict;
