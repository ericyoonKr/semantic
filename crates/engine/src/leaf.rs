// 리프 판정 오케스트레이터 (PROJECT.md §7.1, §10, §11)
// LLM 호출 형태: 명제 1개 + fact slice + authority slice → True/False/Unknown.
//
// 근거 조립(LeafRequest 만들기)은 slices.rs가 한다. 여기는 판정만.
//
// 담을 것:
// - async fn judge_all<C: LeafCache>(reqs: Vec<Prepared>, llm, limiter, cache: &C,
//                                    cancel: CancellationToken) -> HashMap<NodeId, LeafResult>
//   로직:
//   0) Prepared::NoEvidence는 LLM 없이 LeafResult::Unknown { failure: NoEvidence(..) }
//   1) 리프별 cache.get(cache_key) (적중 시 LLM 호출 생략 —
//      두 속도·부분→전체 점증 대화의 핵심, §6.2·§6.3)
//   2) 미스만 JoinSet + LlmLimiter(semaphore)로 fan-out (§10)
//      각 태스크: tokio::select! { cancel.cancelled() => 중단, 판정 => ... }
//      (문서 수정판 도착 시 server::session_actor가 cancel 발동)
//   3) 응답 JSON { value: "True|False|Unknown", basis_clause_id, quote } 파싱 →
//      True/False이고 basis_clause_id가 fact slice 안에 있으면
//        LeafResult::Judged { value, anchor: 그 조항의 anchor, quote }
//      basis_clause_id가 없거나 fact slice 밖이면 → Unknown { failure: NoEvidence(..) }
//        (근거 조항 없는 True/False는 만들지 않는다 — anchor.rs 규칙)
//      LlmError는 FailureReason으로 변환해 Unknown (§11 — 기본값 채움 금지)
//   4) 모순 검출: 긍정/부정 쌍이 둘 다 True → 두 리프 재판정(1회) →
//      여전히 충돌이면 둘 다 Unknown + Contradiction
//   5) 확정 결과를 cache.put (Unknown 중 일시 오류성(timeout)은 저장 안 함 —
//      재시도 여지, NoEvidence·Contradiction은 저장)
//   6) 동시 미스 이중 호출 방지는 LeafCache 구현(server의 moka get_with)이 맡는다 (§10)
