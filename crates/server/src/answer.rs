// ⑩ 판정 문장 조립 (PROJECT.md §5 ⑩, §6.3, §11)
// engine::Verdict가 유일한 판정 입력 — 여기서 판정을 만들지 않는다.
// 만든 문장은 POST /sessions/{sid}/judge 응답의 text로 ④에 간다. ④는 이 문장을
// 고치지 않고, 채팅 창구가 화면에 원문 그대로 따로 보낸다 (VerdictReady 이벤트).
//
// 담을 것:
// - fn render_verdict(v: &Verdict, graph: &ClaimGraph) -> String
//   템플릿 규칙:
//   1) 결론: Established/NotEstablished/Reserved를 판정 범위와 함께 —
//      ★ 자동 문구: "이 판정은 {scope.description} 서브트리에 한정되며,
//        그 밖의 요건은 판정하지 않았음" (처분성 인정 ≠ 취소 인용)
//   2) 불성립이면 unsat_core의 노드·이유 나열 (LLM 사후 설명보다 우선)
//   3) unknown_leaves 전부 표면화: "R4_고의과실: 업로드 문서에서 근거 조항을
//      찾지 못함 — 판정 불가" — 생략 금지 (§11 종착점)
//   4) assumed 값들은 「문서 근거 없음·가정」 표시
//   5) 근거 인용: LeafResult.anchor(clause_id·페이지·좌표) + authority 출처
//
// 서술형 답변과 일반 RAG 답변은 ④가 만든다 — 이 서버에는 없다.
