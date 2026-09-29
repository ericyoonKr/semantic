// checks — 결정론 검증 (PROJECT.md §2-1, §6.1)
// "결정론적으로 계산 가능한 것은 LLM에 묻지 않는다" — 그 목록이 이 crate다.
//
// 담을 것:
// - struct Finding { kind, message, anchors: Vec<Anchor>, severity }
// - fn run_all(doc: &Document, clauses: &[Clause]) -> Vec<Finding>
//   (sums/refs를 순서대로 돌리고 결과를 합친다. dates·periods는 Finding을 내지 않는
//    계산 도구라 run_all에 들어가지 않는다 — sums·refs·engine이 부른다)
//
// 규칙:
// - 전부 동기 순수 함수. tokio·I/O·LLM 의존 금지 — 단위 테스트와
//   정확도 실측이 런타임 없이 가능해야 한다 (§10).
// - 문서 버전이 바뀌면 이 crate는 전체 재실행된다 (빠른 경로, §6.2).
//   LLM 리프(느린 경로)와 달리 캐시하지 않는다.

pub mod dates;
pub mod periods;
pub mod refs;
pub mod sums;
