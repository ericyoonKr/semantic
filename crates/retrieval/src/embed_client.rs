// ② 추론 서버(임베딩) HTTP 클라이언트 (PROJECT.md §5 ②)
// 추론 서버는 infer/의 llama.cpp(--embedding) 또는 onnxruntime 프로세스.
//
// 담을 것:
// - struct EmbedClient { http, base_url, model, dim }
// - async fn embed_one(&self, text: &str) -> Result<Vec<f32>, RetrievalError>
// - async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, RetrievalError>
//     (업로드 문서 청크는 배치로 — 청크 수십~수백 개를 한 번에)
//
// ★★ 착수 전 실측 1 (§12): 코퍼스의 저장 임베딩(조문 30만·판례 412만 행)이
//   어떤 모델로 만들어졌는지 확인하고 반드시 같은 모델을 띄운다.
//   다르면 벡터 검색이 성립하지 않는다. dim 불일치는 여기서 즉시 에러로.
//
// 서버 다운 시: RetrievalError::EmbedderDown → search()가 FTS 단독 강등 (§11).
