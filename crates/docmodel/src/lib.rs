// docmodel — 통합 문서 모델 (PROJECT.md §8)
//
// 파서가 셋(docx/hwpx/pdf)이어도 출력은 이 crate의 타입 하나로 수렴한다.
// clause_id 부여, 결정론 검증(checks), RAG 청킹(retrieval::upload_index),
// 엔진의 fact slice(engine::leaf)가 전부 이 모델 위에서 돈다.
//
// 원칙: 모든 추출 값은 Anchor(출처 좌표)를 달고 다닌다.
//       anchor 없는 값은 Z3·산술 검증에 들어갈 수 없다.
//
// 이 crate는 순수 타입 + 순수 함수만. tokio·I/O 의존 금지.

pub mod anchor;
pub mod block;
pub mod clause;