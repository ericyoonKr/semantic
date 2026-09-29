/ llm — Rust 쪽 LLM 호출 공통층 (PROJECT.md §5 ③·⑤, §7.1 호출 규칙)
//
// Rust 쪽 LLM 호출(리프 판정, 쿼리 재작성)이 이 crate를 거친다.
// 도구 선택·서술 생성은 ④가 LangChain으로 한다 — 이 crate에는 없다.
//
// 공통 계약 (가이드북 그대로):
// - temperature = 0, 짧은 max_tokens, timeout, 지수 backoff(횟수 제한)
// - strict JSON: 파싱 실패 시 1회 재요청, 그래도 실패면 에러 반환
//   (호출자가 Unknown으로 강등 — 기본값을 채워주지 않는다)
// - 단계 사이에 CoT·대화 이력을 넘기지 않는다. typed 요청/응답만.
//
// 담을 것: pub mod client; pub mod limits;
// - enum LlmError { Timeout, RateLimited, Http(status), ParseFailed, Cancelled }
//   → engine::leaf가 FailureReason으로 변환한다.

pub mod client;
pub mod limits;
