// 세션 actor — 세션 안의 업로드·판정은 직렬, 세션 사이는 병렬 (PROJECT.md §10)
//
// 담을 것:
// - enum SessionEvent { Upload(파일, 응답채널), Judge(요청, 응답채널), Shutdown }
// - struct SessionHandle { tx: mpsc::Sender<SessionEvent>, cancel: CancellationToken }
// - 세션 레지스트리: DashMap<SessionId, SessionHandle> 또는 Mutex<HashMap>
//   세션 ID는 ④가 만든다 — 처음 보는 sid로 요청이 오면 그때 actor를 기동한다.
// - async fn run_session(rx, state: AppState, session_id)
//     단일 소비 루프: 이벤트를 도착 순서대로 처리한다.
//
// 로직:
// - Upload 처리: upload.rs 파이프라인 호출 → 업로드 요약을 응답 채널로.
//   ★ 문서 수정판 도착 시: 진행 중이던 Judge(리프 판정 fan-out)의
//   CancellationToken을 cancel하고 새 토큰 발급 (§10). 취소된 Judge는
//   「판정 불가: 문서 수정판 도착」으로 응답한다.
// - Judge 처리: engine(서브트리 컴파일 → 리프 판정 → Z3) → answer::render_verdict
//   → 응답 채널로.
// - 유휴 타임아웃으로 actor 자체 종료(레지스트리에서 제거). 세션 산출물은 sessdb에
//   있으므로 다시 기동하면 이어진다.
