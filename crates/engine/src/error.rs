use std::fmt;

use thiserror::Error;

use crate::schema::{NodeId, Relation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountKind {
    Nodes,
    Edges,
    Leaves,
}

impl fmt::Display for CountKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            CountKind::Nodes => "노드",
            CountKind::Edges => "간선",
            CountKind::Leaves => "리프",
        };
        f.write_str(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GraphIssue {
    #[error("노드 ID가 겹친다: {0}")]
    DuplicateNode(NodeId),
    #[error("간선 끝 노드가 없다: {parent} -> {child}")]
    UnknownEndpoint { parent: NodeId, child: NodeId },
    #[error("간선이 겹친다: {parent} -> {child}")]
    DuplicateEdge { parent: NodeId, child: NodeId },
    #[error("{} 관계 층에 순환이 있다", .0.as_str())]
    LayerCycle(Relation),
    #[error("관계 층을 섞은 순환이 있다")]
    MixedCycle,
    #[error("LEAF에 proposition이 없다: {0}")]
    LeafWithoutProposition(NodeId),
    #[error("LEAF에 ask가 없다: {0}")]
    LeafWithoutAsk(NodeId),
    #[error("LEAF에 element 자식이 있다: {0}")]
    LeafWithElements(NodeId),
    #[error("AND·OR 노드에 element 자식이 없다: {0}")]
    GateWithoutElements(NodeId),
    #[error("negation_of가 가리키는 노드가 없다: {0}")]
    NegationTargetMissing(NodeId),
    #[error("{kind} 수가 기대와 다르다: 기대 {expected}, 실제 {actual}")]
    CountMismatch {
        kind: CountKind,
        expected: usize,
        actual: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    #[error("요건 트리 검증 실패: {}", join_issues(.0))]
    Graph(Vec<GraphIssue>),
    #[error("노드가 없다: {0}")]
    UnknownNode(NodeId),
    #[error("판정 범위의 루트가 하나가 아니다: {0:?}")]
    NotSingleRoot(Vec<NodeId>),
    #[error("리프 판정 결과가 없다: {0}")]
    MissingLeafResult(NodeId),
}

fn join_issues(issues: &[GraphIssue]) -> String {
    issues
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}
