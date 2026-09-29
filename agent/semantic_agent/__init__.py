# semantic_agent — ④ 채팅 에이전트 (PROJECT.md §4, §5 ④, §9.1)
# Deep Agents(LangGraph·LangChain) 위에서 돈다. 검색·판정·문서 파싱은 하지 않고
# ① Rust 내부 도구 API를 HTTP로 부른다.
#
# 모듈: config(설정), memory(세션 기억 = LangGraph 체크포인트), tools(Rust 도구 API 호출),
#       prompts(시스템 프롬프트), graph(에이전트 조립),
#       app(채팅 창구 엔드포인트), stream(SSE 이벤트 변환), sessions(세션별 락·업로드 우선 취소)
#
# 실행: cd agent && uv run uvicorn semantic_agent.app:app --host 127.0.0.1 --port 8000
