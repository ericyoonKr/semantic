// WriteBatch 원자 쓰기 (PROJECT.md §9.2, §11)
//
// 담을 것:
// - fn put_upload(&self, session, file_id, doc, clauses, index, findings)
//       -> Result<(), StorageError>
//   업로드 한 건의 산출물을 하나의 rocksdb::WriteBatch로 커밋 —
//   중간에 죽어도 "문서는 있는데 조항·색인이 없는" 반쪽 상태가 안 생긴다.
//
// 규칙:
// - WAL 기본값 유지 (내구성).
// - 실패는 StorageError로 위로 — server가 세션에 오류로 표면화 (§11).
