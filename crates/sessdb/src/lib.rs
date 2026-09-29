// sessdb — ⑧ RocksDB: Rust 쪽 세션 산출물과 리프 판정 캐시 (PROJECT.md §9.2)
// 코퍼스는 PostgreSQL, 대화 기록은 ④의 LangGraph 체크포인트(§9.1), 업로드 문서 파싱 결과와
// 리프 판정 캐시는 RocksDB — 역할 분리.
//
// 담을 것:
// - pub struct SessDb(Arc<rocksdb::DB>)   // 핸들은 스레드 세이프, Arc 공유 (§10)
// - fn open(path) -> Result<SessDb, StorageError>
//     CF 2개를 열며 없으면 생성: "state", "leaf_cache"
//
// 규칙 (§10, §11):
// - 호출은 µs~ms라 대부분 그대로 불러도 되지만, 긴 iterator 순회만 spawn_blocking.
// - 쓰기 실패는 세션에 오류로 반환한다 — 삼키지 않는다.
// - 업로드 한 건의 산출물처럼 함께 움직여야 하는 쓰기는 batch.rs의 WriteBatch로.

pub mod batch;
pub mod cf;
