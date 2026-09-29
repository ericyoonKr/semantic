// 내부 도구 API (PROJECT.md §5 ①) — 부르는 쪽은 agent/semantic_agent/tools.py·app.py
//
// 담을 것:
// - POST   /sessions/{sid}/files         → multipart 파일 → session_actor에 Upload →
//                                          업로드 요약 JSON (file_id, 추출 범위,
//                                          추출 불가 경고, Finding 개수)
// - POST   /sessions/{sid}/upload_search → { query, k } → 업로드 문서 top-k
//                                          (clause_id·anchor 포함)
// - GET    /sessions/{sid}/files/{file_id}/clauses?ids= → 조항 원문 + anchor
// - POST   /search/laws, /search/cases   → { query, k } → retrieval top-k
//                                          + 출처 필드 + degraded
// - GET    /catalog                      → 요건 노드 카탈로그 (AppState 캐시)
// - POST   /sessions/{sid}/judge         → { claim_type, node_id, assumptions } →
//                                          session_actor에 Judge →
//                                          { verdict: Verdict 요약,
//                                            text: answer::render_verdict 결과 }
// - DELETE /sessions/{sid}               → actor 종료 + sessdb delete_session
//
// 규칙:
// - 핸들러는 얇게: 검증 → (세션 작업이면 actor 채널 송신) → JSON 응답.
//   검색(/search/*)은 세션 상태를 건드리지 않으므로 actor를 거치지 않는다.
// - 업로드 크기 상한, 지원 형식 안내(§12 결정 3)를 여기서 조기 검사.
// - 판정 실패도 HTTP 오류가 아니라 Verdict의 Unknown·판정 불가로 돌려준다 (§2 원칙 2).
