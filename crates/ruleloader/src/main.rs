// ruleloader — rules/*.yaml → 검증 → claim_nodes/claim_edges 적재 (PROJECT.md §7.3)
//
// clap CLI:
//   ruleloader load rules/claim_취소소송.yaml     # 1개 적재
//   ruleloader load-all rules/                   # 전부
//   ruleloader check rules/claim_취소소송.yaml    # 적재 없이 검증만
//
// 로직:
// 1) serde_yaml → engine::schema 타입 (ClaimNode/ClaimEdge 목록)
// 2) 검증 — 하나라도 실패하면 ★부분 적재 금지, 전체 중단 (§11):
//    - DAG 여부 (engine::graph 가드 재사용)
//    - 기대 노드·엣지·리프 수 (yaml의 expected 블록과 대조)
//    - LEAF에 proposition 존재, 비-LEAF에 자식 존재
//    - 리프 문장 형식 검사 가능 범위: 금지어 목록(결론성 평가어 — "위법하다",
//      "인정된다" 등)이 proposition에 들어 있으면 경고/거부
//    - burden 미정 노드 거부 (§7.5 결정 전 트리 적재 방지)
// 3) 트랜잭션 하나로 DELETE(해당 claim_type) + INSERT 전량 — 부분 상태 금지.
//
// 적재 후: 노드 카탈로그(GET /catalog — ④ judge 도구 설명에 들어감)가 바뀌므로
// server와 agent 재기동 필요 사실을 출력.

fn main() {
    // clap 파싱 → tokio 런타임 → 서브커맨드 분기.
}
