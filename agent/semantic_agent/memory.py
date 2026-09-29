# 세션 기억 — LangGraph 체크포인트를 PostgreSQL에 저장 (PROJECT.md §9.1)
#
# 기억하는 것 (세션 ID = thread_id 하나에 전부 묶인다):
# - state["messages"]: 사용자 질문, 에이전트 답변(AIMessage), 도구 호출과 도구 결과(ToolMessage),
#   업로드 요약(app.py가 넣음). 곧 「채팅에서 답한 것 + 에이전트가 읽은 것」.
#   체크포인트는 에이전트가 한 단계를 끝낼 때마다 저장된다.
# - state["files"]: StateBackend(create_deep_agent 기본값)의 파일.
#     /large_tool_results/{tool_call_id} — 2만 토큰 넘는 도구 결과의 원문
#     /conversation_history/{id}.md      — 요약 때 모델 입력에서 빠진 앞부분 대화의 원문
#
# 담을 것:
# - async def open_checkpointer(settings) -> AsyncPostgresSaver
#     pool = psycopg_pool.AsyncConnectionPool(settings.checkpoint_pg_url, max_size=settings.pg_pool_max,
#                kwargs={"autocommit": True, "prepare_threshold": 0, "row_factory": dict_row}, open=False)
#     await pool.open(); saver = AsyncPostgresSaver(conn=pool); await saver.setup()  # 테이블 생성
#     연결 옵션 세 개는 AsyncPostgresSaver.from_conn_string이 쓰는 값과 같다 (aio.py:82).
# - async def delete_session(saver, session_id): await saver.adelete_thread(session_id)
#
# 확인한 동작 (2026-09-29, deepagents 0.7.19 소스 + 가짜 모델 시험):
# - 요약(기본으로 켜짐)은 state["messages"] 원본을 지우지 않는다. 모델에 보내는 입력만
#   「요약 + 최근 메시지」로 바꾼다 (deepagents/middleware/summarization.py:1805).
#   작동 기준: 모델 문맥 한도의 85% (한도를 모르는 모델이면 17만 토큰).
# - 2만 토큰(8만 자) 넘는 도구 결과는 원문을 /large_tool_results/에 저장하고, 메시지에는
#   앞뒤 일부와 경로만 남긴다 (filesystem.py:1727). 시험에서 106,890자 원문이 그대로 저장됐고
#   메시지에는 1,204자가 남았다 — 원문을 다시 보려면 에이전트가 read_file을 불러야 한다.
# - 같은 checkpointer로 에이전트 객체를 새로 만들어 같은 thread_id로 부르면, 앞 대화의 답변과
#   도구 결과가 모델 입력에 들어간다 (in-memory 체크포인트로 시험).
# - PostgreSQL 체크포인트로 프로세스를 다시 띄워도 이어진다 (2026-09-29, 위 연결 방식 그대로,
#   pgvector/pg16 시험용 컨테이너): 첫 질문 프로세스가 끝난 뒤 새 프로세스가 같은 thread_id로
#   질문하자 모델 입력에 첫 답변·도구 결과가 들어 있었고, 106,890자 도구 결과의 SHA-256이
#   원문과 같았다. adelete_thread 뒤에는 메시지·파일 0개, checkpoints·checkpoint_writes·
#   checkpoint_blobs 세 테이블의 해당 행 0.
# - 하위 에이전트(task 도구)가 읽은 도구 결과는 메인 대화에 남지 않는다. 마지막 답변 하나와
#   files만 돌아온다 (subagents.py:717). → prompts.py 규칙 5, graph.py 규칙.
#
# 규칙:
# - 세션 사이 공유 기억(LangGraph Store)은 쓰지 않는다 — 기억 범위는 세션 하나.
# - 한 thread_id에 실행 두 개를 동시에 돌리지 않는다. 같은 체크포인트에서 갈라진 두 실행 중
#   한쪽 기록이 최신 상태에서 빠질 수 있다. sessions.py가 세션별 asyncio.Lock으로 막는다.
# - 체크포인트 쓰기 실패는 그 질문을 Error로 끝낸다 — 저장 못 한 답을 기억한 것처럼 두지 않는다 (§11).
