// OpenAI 호환 /v1/chat/completions 클라이언트 (PROJECT.md §5)
// ★ 어떤 모델·어디(외부 API/로컬)로 갈지는 §12 결정 2 — base_url·모델명을
//   설정으로 받아 양쪽 다 되게 한다.
//
// 담을 것:
// - struct LlmClient { http: reqwest::Client, base_url, api_key, model }
// - async fn complete_json<T: DeserializeOwned>(&self, system: &str, user: &str)
//       -> Result<T, LlmError>
//   로직:
//   1) response_format=json(지원 시) + temperature 0 + max_tokens 상한으로 요청
//   2) timeout — reqwest 타임아웃과 별개로 전체 마감(tokio::time::timeout)
//   3) 429/5xx → 지수 backoff 재시도 (기본 3회)
//   4) 본문에서 JSON 추출 → serde 파싱. 실패 시 "JSON만 출력" 지시 붙여 1회 재요청.
//   5) 최종 실패는 LlmError::ParseFailed — 절대 기본값 T를 만들어내지 않는다.
//
// 계측: 호출마다 지연·재시도 횟수·모델명을 tracing으로 남긴다
//       (평가 표본 실측 §12 미논의 2의 재료).
