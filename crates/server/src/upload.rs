// 업로드 파이프라인 (PROJECT.md §6.1)
//
// 흐름:
// 1) parsers::detect → parse
//    - PDF는 pdfium 전용 스레드 채널로 (main.rs에서 기동), 나머지는 spawn_blocking
//    - ParseError::Unsupported/EmptyText → 사용자에게 「추출 불가·판정 불가」 명시 (§11)
// 2) docmodel::clause::assign_clauses → clause_id 부여
// 3) checks::run_all → 결정론 Finding들 (spawn_blocking)
//    괘선 없는 표 우회: 격자 재구성 실패 페이지를 이미지 렌더 →
//    멀티모달 LLM 표 추출 → 신뢰 등급 하락 표시 + 페이지·좌표 인용 (§8)
// 4) retrieval::embed_client.embed_batch(clause 텍스트들) → upload_index::build
// 5) sessdb 저장 (WriteBatch): Document·Clauses·UploadIndex 사본·Finding들
// 6) 업로드 요약을 응답으로: file_id, 추출 범위, 경고(warnings), Finding 개수
//    ④가 이 요약을 대화에 넣고(에이전트가 올라온 파일을 기억) 사용자에게도 보낸다.
//
// 문서 버전: 같은 세션에 재업로드되면 새 file_id(내용 해시) — checks는 전체
// 재실행(빠른 경로), 리프는 cache_key 변경분만 재판정된다 (§6.2).
