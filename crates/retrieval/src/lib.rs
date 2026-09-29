// retrieval — ③ 검색 코어 + ⑦ 업로드 문서 RAG (PROJECT.md §5)
//
// 흐름: rewrite(쿼리 재작성+캐시) → embed(② 호출) → RRF SQL → 상위 k
//       (리랭커는 후순위 — 기존 실측에서 250ms 개선 항목이지 필수 경로 아님)
//
// 담을 것:
// - struct SearchHit { text, law_name/case_ref, score, source }  // 법령·판례 공용
// - async fn search(pool, q, filters, k) -> Result<Vec<SearchHit>, RetrievalError>
// - 강등 규칙 (§11): 재작성 실패 → 원 질의로 / ② 다운 → FTS 단독 + 강등 표시
//   → 응답에 degraded: bool을 싣는다. ④의 tools.py가 결과 첫 줄에 강등 사실을 적어
//     에이전트가 답변에 밝힌다.
//
// 병렬 (§10): 재작성과 질의 임베딩은 tokio::join!.
//   업로드 검색 ∥ DB 검색은 engine::slices가 리프마다 join!으로 부른다 (일반 질문에서는
//   ④가 도구를 따로 부른다).

pub mod embed_client;
pub mod rewriter;
pub mod sql;
pub mod upload_index;
