// server — ① 내부 도구 API 서버 진입점 (PROJECT.md §5 ①, §10)
// 채팅 창구는 agent/(④, Python)다. 이 서버는 127.0.0.1에만 바인드하고 ④만 부른다.
// 대화 기록은 여기 없다 — ④의 LangGraph 체크포인트에 있다 (§9.1).
//
// 부팅 순서:
// 1) tracing_subscriber 초기화 (env-filter)
//    요건 DAG RW), RocksDB 경로, ② base_url, LLM base_url·key, 동시성 상한,
//    바인드 주소(127.0.0.1)
// 3) sqlx 풀 생성, sessdb::open, retrieval/llm 조립
//    + engine::EngineDeps 조립: engine_deps.rs의 UploadFacts·CorpusAuthority·LeafStore
//    + 요건 DAG: claim_nodes/claim_edges를 sqlx로 읽어 engine::graph::ClaimGraph::from_rows
//      (engine은 DB를 모른다) → AppState에 보관
//    + 노드 카탈로그(claim_type별 node_id·description 목록)를 한 번 만들어 AppState에
//      캐시 — GET /catalog로 ④에 준다 (judge 도구 설명에 들어감)
// 4) pdfium 전용 스레드 기동: std::thread 1개 + mpsc 채널 —
//    모든 PDF 파싱 요청은 이 채널로 직렬화 (pdfium은 스레드 세이프 아님, §10)
// 5) axum Router(routes.rs) + tower_http CatchPanicLayer(패닉→500 하나로, §11)
//    + TraceLayer, 바인드·serve
//
// 모듈: routes(도구 API), session_actor(세션 안 업로드·판정 직렬화),
//       upload(업로드 파이프라인), answer(⑩ 판정 문장 조립),
//       engine_deps(engine 트레이트 구현)

mod answer;
mod engine_deps;
mod routes;
mod session_actor;
mod upload;

fn main() {
    // tokio 런타임 구성 후 위 부팅 순서대로.
}
