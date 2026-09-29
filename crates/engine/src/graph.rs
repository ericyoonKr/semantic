// 요건 DAG 적재·검증·서브트리 추출 — NetworkX 몫의 Rust 이식 (PROJECT.md §7.3)
//
// 담을 것:
// - struct ClaimGraph { g: petgraph::graph::DiGraph<ClaimNode, Relation>,
//                       by_id: HashMap<NodeId, NodeIndex> }
// - fn from_rows(nodes: Vec<ClaimNode>, edges: Vec<ClaimEdge>) -> Result<ClaimGraph, EngineError>
//     DB는 여기서 읽지 않는다 — server(부팅 때)·ruleloader(적재 전 검증)가 sqlx로 읽어
//     행 목록을 넘긴다. engine은 sqlx를 모른다 (deps.rs의 이유와 같음). 전부 동기.
// - 적재 가드 — 하나라도 실패하면 부분 적재 금지, 전체 중단 (§11):
//   1) petgraph::algo::is_cyclic_directed == false (DAG 보장 = 컴파일 종료 보장)
//   2) 청구유형별 기대 노드·엣지·리프 수 고정 검사 (EXPECTED 테이블)
//   3) LEAF 노드에 proposition 존재, 비-LEAF에 자식 존재
// - fn subtree(&self, root: NodeId) -> ClaimGraph
//     임의 노드를 루트로 서브트리 추출 — 부분 질문(§6.3)의 근거.
//     "처분성만" → S1 서브트리, "제소기간만" → S3 하나.
// - 공유 리프 허용: 같은 리프 묶음(처분성)을 취소소송·무효확인 트리가 공유 —
//   node_id가 전역 유일하면 그래프 간 공유는 leaf_cache 차원에서 자동으로 된다.
// - fn topo_order(&self) -> Vec<NodeId>   // 위상정렬 (컴파일 순서)
