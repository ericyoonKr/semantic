# SSE 이벤트 변환 — agent.astream 출력 → 화면 이벤트 (PROJECT.md §5 ④, §2 원칙 1)
# app.py(엔드포인트)와 나눈 이유: 어떤 이벤트를 어떻게 보여 줄지는 웹 프론트에 맞춰 따로 바뀐다.
#
# 담을 것:
# - async def to_sse(agent, question, cfg) -> AsyncIterator[str]   # "event: ...\ndata: ...\n\n"
#     agent.astream({"messages": [{"role": "user", "content": question}]}, cfg,
#                   stream_mode=["messages", "updates"])
# - 이벤트 (옛 Rust sse.rs의 StreamEvent를 옮김):
#     Status(도구 호출 시작·끝) / Token(답변 조각) /
#     VerdictReady(judge의 ToolMessage 원문 — 모델이 서술하며 바꿔도 판정 원문이 화면에 남는다) /
#     Warning(검색 강등·추출 불가) / Error(메시지, 그 뒤 스트림 닫음) / Done
#
# 규칙:
# - Token은 요약·하위 에이전트의 모델 호출 토큰이 섞이지 않게 메인 에이전트 것만 보낸다
#   (거를 때 쓸 metadata 키는 아직 확인 안 함).
# - 체크포인트 쓰기 실패 등 실행 중 예외는 Error 이벤트로 알리고 스트림을 닫는다 (§11).
