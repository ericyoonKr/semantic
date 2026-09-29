// Column family별 key 설계와 접근자 (PROJECT.md §9.2)
//
// | CF          | key                          | value |
// |-------------|------------------------------|-------|
// | state       | {session_id}/doc/{file_id} 등| Document·Clause들·UploadIndex 사본·Finding (serde_json) |
// | leaf_cache  | {cache_key}                  | LeafResult의 serde_json 바이트 (세션 무관 전역 — 같은 문서+같은 리프면 재사용) |
//
// 대화 기록·판정 결과·세션 메모는 여기 두지 않는다 — ④의 LangGraph 체크포인트에
// 대화와 도구 결과(judge 응답 포함)가 저장된다 (§9.1).
//
// 담을 것:
// - fn get_state / put_state (serde_json 직렬화)
// - fn list_files(&self, session) → state CF prefix iterator ({session}/doc/)
// - fn get_leaf(&self, cache_key) -> Result<Option<Vec<u8>>, StorageError> / fn put_leaf(&self, cache_key, &[u8])
//   값은 바이트 — LeafResult는 engine 타입이라 sessdb는 모른다. 직렬화는 server/src/engine_deps.rs가 한다.
// - fn delete_session(&self, session) — state CF에 delete_range({session}/ .. {session}0)
//   leaf_cache는 세션과 무관한 전역 캐시라 지우지 않는다.
