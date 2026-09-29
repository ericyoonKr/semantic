// 쿼리 재작성 — 일상어 → 법률어 (PROJECT.md §5 ③)
// 근거: 코퍼스에 일상어 토큰이 거의 없어 BM25·벡터 모두 1차 후보에
// 정답이 안 들어온다 → LLM 1회로 법률 키워드로 정제 후 검색.
//
// 담을 것:
// - async fn rewrite(client: &LlmClient, cache: &moka::future::Cache<String, String>, q: &str)
//       -> RewriteOutcome { rewritten: String, used_llm: bool }
// 로직:
// 1) 정규화한 q를 키로 moka 캐시 조회 (기존 실측: 이 호출이 지연 대부분 ~2.8s,
//    캐시 적중 시 ~0ms — 캐시 실효가 가장 큰 지점)
// 2) 미스면 LLM 호출: "다음 질문을 한국 법령·판례 검색용 키워드로" → 캐시 저장
// 3) 실패 시 원 질의 그대로 반환 (강등이지 중단이 아님, §11)
//
// 캐시 스탬피드: moka get_with로 동일 키 동시 미스를 하나로 합류 (§10).
