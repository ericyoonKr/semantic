// 요건 트리 타입 — claim_nodes/claim_edges 매핑 (PROJECT.md §7.3)
//
// 담을 것:
// - struct NodeId(String), ClaimType(String)
// - enum GateType { Leaf, And, Or }
// - enum Relation { Element, Defense, Rebuttal }   // 요건/항변/재항변
// - enum Burden { ... }
//   ★ §7.5 결정 사항: 민사식 PLAINTIFF/DEFENDANT가 아니라 행정소송 의미로
//   재정의해야 한다 — 소송요건=직권조사 / 본안 적법성=처분청 / 재량 일탈·남용=원고.
//   이 귀속이 Unknown 리프의 불이익 방향을 정한다. 결정 전에는 트리를 그리지 않는다.
// - struct ClaimNode { id, claim_type, description, gate: GateType, burden,
//                      is_leaf, proposition: String,      // 리프 판정 문장
//                      arithmetic: Option<ArithSpec> }    // 산술 리프 (제소기간 등)
// - struct ClaimEdge { parent: NodeId, child: NodeId, relation: Relation }
// - enum Trit { True, False, Unknown }        // LLM 응답을 읽을 때만 쓴다
// - enum LeafResult {
//       Judged  { value: bool, anchor: Anchor, quote: String },  // 판정된 리프 — Anchor 필수
//       Unknown { failure: FailureReason },                       // 판정 불가 — 이유 필수
//   }
//     ★ 판정된 값과 Unknown을 변형으로 가른 이유: True/False인데 anchor가 없는 값을
//       타입상 만들 수 없게 한다 (anchor.rs 규칙 — anchor 없는 값은 Z3에 못 들어간다).
//     failure(Unknown의 이유)는 최종 보고서까지 살아 올라간다 (§11) —
//     unsat_core가 "왜 불성립"을 대듯 FailureReason이 "왜 판정 불가"를 댄다.
// - enum ArithSpec {                           // 산술 리프 (claim_*.yaml의 arithmetic)
//       WithinDays  { from: DateSlot, to: DateSlot, days: u32 },    // 예: 안 날부터 90일
//       WithinYears { from: DateSlot, to: DateSlot, years: u32 },   // 예: 있은 날부터 1년
//   }
//     DateSlot = 문서에서 뽑을 날짜 이름(처분이 있음을 안 날, 제소일 …).
//     마감일 계산(초일 불산입, 역에 따른 연 계산, 말일 공휴일)은 checks::periods가 하고,
//     Z3에는 마감일·제소일의 ordinal 정수만 들어간다 (§7.2).
// - enum FailureReason { LlmTimeout, LlmParseFailed, Contradiction, NoEvidence(설명), ... }
// - fn cache_key(rule_id, fact_slice_hash, authority_snapshot_hash) -> String  // sha2
//
// serde 파생 — ruleloader의 YAML 검증과, server의 LeafCache 구현이 sessdb leaf_cache에
// serde_json으로 저장할 때 같은 타입을 쓴다 (sessdb는 LeafResult 타입을 모르고 바이트만 저장).
