# 채팅 창구 — FastAPI 엔드포인트와 부팅 (PROJECT.md §5 ④, §6, §10)
# 사용자(웹 프론트)가 만나는 유일한 HTTP 서버. ① Rust 서버는 127.0.0.1에서 이 프로세스만 부른다.
# SSE 이벤트 변환은 stream.py, 세션별 락·취소는 sessions.py가 맡는다. 여기는 엔드포인트만.
#
# 부팅 (lifespan):
# 1) Settings 로드 → memory.open_checkpointer (setup 포함)
# 2) httpx.AsyncClient(base_url=rust_api_url, timeout=rust_timeout_s) 하나 →
#    GET /catalog로 judge 설명을 채워 tools 생성
# 3) graph.build_agent, sessions.SessionRegistry() → app.state에 보관. 종료 때 풀·클라이언트 닫기.
#
# 담을 것:
# - POST /session → session_id = uuid4 문자열 (= thread_id).
# - POST /session/{id}/upload (multipart) → sessions.run_upload로:
#     Rust POST /sessions/{id}/files로 그대로 넘김 →
#     돌아온 업로드 요약(file_id, 추출 범위, 추출 불가 경고, Finding 개수)을
#     agent.aupdate_state(cfg, {"messages": [HumanMessage("[업로드] " + 요약)]})로 대화에 넣는다
#     → 에이전트가 이후 질문에서 어떤 파일이 올라왔는지 안다. 요약은 사용자에게도 그대로 보낸다.
# - POST /session/{id}/ask {question} → sessions.run_ask 안에서 stream.to_sse(...)를
#     StreamingResponse(text/event-stream)로 돌려준다.
# - GET /session/{id}/history → agent.aget_state(cfg).values["messages"]를 화면용으로 변환.
#     재접속·새로고침 때 이 목록으로 채팅을 다시 그린다. 스트리밍 도중 끊긴 답변은
#     그 단계가 끝나 체크포인트에 저장된 뒤부터 보인다.
# - DELETE /session/{id} → memory.delete_session + Rust DELETE /sessions/{id}
#
# 규칙:
# - cfg = {"configurable": {"thread_id": session_id}}
