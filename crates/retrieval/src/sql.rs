//
// 담을 것:
// - const RRF_SQL: &str  또는 조립 함수:
//     WITH fts_cand AS (...),      -- 전문검색 후보 N건
//          vec_cand AS (...)       -- pgvector <=> 코사인 후보 N건
//     SELECT ... RRF 결합 (1/(k+rank) 합산) ORDER BY ... LIMIT $k
// - law_type / status(현행) 필터 바인딩
//
// ★ 착수 전 실측 (§12 실측 2): 기존 DB의 한국어 FTS 인덱스가 어떤 방식인지
//   (mecab 계열 확장 / pg_trgm / bigram) 확인하고 같은 방식으로 질의해야 한다.
//   방식이 확인되기 전에는 이 파일의 fts_cand를 확정하지 않는다.
//
// ★ 실측 1: 질의 임베딩 모델은 저장된 임베딩 세트와 동일해야 한다 — embed_client 주석 참조.
