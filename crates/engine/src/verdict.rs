// 판정 결과 조립 (PROJECT.md §7.2, §6.3, §11)
//
// 담을 것:
// - enum Outcome { Established, NotEstablished, Reserved }
//     Z3 sat → Established (전제: 사실 전부 고정 시)
//     unsat  → NotEstablished
//     unknown → Reserved (결론 유보 — unsat 취급 금지, 삼값을 타입으로 강제 §11)
// - struct Verdict {
//       outcome: Outcome,
//       scope: NodeId,                          // ★ 판정 범위 — 어느 서브트리에 한정인지.
//                                               //   "처분성 인정 ≠ 취소 인용" (§6.3)
//       unsat_core: Vec<(NodeId, String)>,      // 불성립 이유 (LLM 사후 설명보다 우선)
//       unknown_leaves: Vec<(NodeId, FailureReason)>,  // 판정 불가 목록+이유
//       assumed: Vec<(NodeId, bool)>,           // 가정 질문에서 고정한 값 —
//                                               //   「문서 근거 없음」 표시 대상
//       model_hint: Option<Vec<(NodeId, bool)>>,// sat 모델에서 읽은 "성립에 필요한 조합"
//   }
// - fn from_raw(raw: RawVerdict, subtree: &ClaimGraph, leaves: &HashMap<..>) -> Verdict
//
// 규칙:
// - 이 타입이 server::answer(⑩)의 유일한 입력이다. 여기 없는 정보는 답변에
//   나갈 수 없고, 여기 있는 Unknown 목록·가정 표시는 답변에서 생략할 수 없다.
