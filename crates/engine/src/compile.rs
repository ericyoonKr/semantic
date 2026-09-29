// 서브트리 → Z3 식 컴파일·판정 (PROJECT.md §7.2)
// 규칙은 선언 데이터(claim graph)에서 결정론적으로 컴파일한다.
// LLM에게 Z3 식을 생성·실행시키지 않는다.
//
// 담을 것:
// - fn check(subtree: &ClaimGraph, leaves: &HashMap<NodeId, LeafResult>,
//            assumptions: &HashMap<NodeId, bool>) -> RawVerdict
//   전부 동기 함수. 호출자(lib.rs judge)가 spawn_blocking으로 감싼다.
// - struct RawVerdict { result: 삼값(sat/unsat/unknown), core: Vec<NodeId>,
//                       model: Option<Vec<(NodeId, bool)>> }   → verdict.rs가 Verdict로
//
// 로직:
// 1) ★ Context는 이 함수 안에서 생성하고 함수가 끝나면 버린다 (§10 —
//    스레드 간 공유 불가. 판정은 ms 단위라 매번 생성해도 비용 없음).
// 2) 위상정렬 순서로 노드 식 조립:
//    - LEAF → Bool 상수 이름 = node_id
//    - AND/OR → 자식 식들의 And(..)/Or(..)
//    - Relation::Defense → 부모 성립식에 Not(항변식) 결합,
//      Rebuttal → 항변식에 Not(재항변식) 결합 (요건사실론 층 구조)
//    - 산술 리프(ArithSpec) → Int 변수(날짜 ordinal, 금액 정수) 비교식.
//      마감일은 checks::periods가 이미 계산해 ordinal로 넘긴다 — 여기는 정수 비교만.
// 3) 사실 주입: LeafResult::Judged 리프만 s.assert_and_track(var == value, node_id).
//    Unknown 리프는 자유변수로 남긴다 (구속하지 않음). Judged에는 Anchor가 반드시 있으므로
//    anchor 없는 값이 Z3에 들어가는 길이 타입으로 막힌다.
// 4) assumptions(가정 질문 §6.3): 지정 노드를 고정값으로 add.
//    이 값들로 얻은 결론에는 「문서 근거 없음」 표시가 따라간다 (verdict가 담당).
// 5) s.check() 삼값 그대로 반환 + unsat이면 unsat_core 수집,
//    sat이고 자유변수가 있으면 model에서 값 읽기(성립에 필요한 사실 조합).
//
// 모순 검출 지원: 긍정/부정 쌍 리프 공리(unpaid == Not(paid))를 여기서 추가 —
// leaf.rs가 충돌 감지→재판정→Unknown 강등을 주관한다.
