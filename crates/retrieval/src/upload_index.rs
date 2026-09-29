// ⑦ 업로드 문서 RAG — 세션 범위 벡터 검색 (PROJECT.md §5 ⑦)
//
// 담을 것:
// - struct UploadIndex { entries: Vec<(ClauseId, Vec<f32>)>, dim: usize }
// - fn build(clauses: &[Clause], vectors: Vec<Vec<f32>>) -> UploadIndex
// - fn top_k(&self, query_vec: &[f32], k: usize) -> Vec<(ClauseId, f32)>
//     brute-force 코사인 — 문서 1~2개 분량(수백~수천 청크)이라
//     인덱스(pgvector/HNSW)가 필요 없고 이게 가장 빠르고 단순하다.
// - serde 직렬화/복원 — 재접속 대비 사본을 RocksDB state CF에 보관 (sessdb).
//
// 규칙:
// - 청크 = clause (따로 만들지 않는다). RAG 인용과 fact slice 출처가
//   같은 clause_id 좌표를 가리키게 하는 장치 (§6.1).
// - 쓰임 두 갈래: 일반 질문의 컨텍스트 top-k / 엔진 경로에서 리프별
//   fact slice 후보 조항 선별 (§6.2).
// - CPU 연산이므로 동기 함수. 큰 문서면 호출자가 spawn_blocking.
