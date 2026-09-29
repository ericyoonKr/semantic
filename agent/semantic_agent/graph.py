# 에이전트 조립 (PROJECT.md §4)
#
# 담을 것:
# - def build_agent(settings, checkpointer, tools) -> CompiledStateGraph
#     model = ChatOpenAI(base_url=settings.llm_base_url, api_key=settings.llm_api_key,
#                        model=settings.llm_model, temperature=0)
#     return create_deep_agent(model=model, tools=tools, system_prompt=SYSTEM_PROMPT,
#                              checkpointer=checkpointer)
# - 에이전트는 부팅 때 한 번 만든다. 세션 구분은 실행마다 넘기는 thread_id가 한다.
#
# 규칙:
# - backend는 넘기지 않는다 → 기본값 StateBackend: 파일이 세션 상태에 들어가 체크포인트와 함께
#   저장된다 (memory.py). FilesystemBackend(실제 디스크)로 바꾸면 파일이 체크포인트에 들어가지 않아
#   세션 기억에서 빠진다.
# - 하위 에이전트: general-purpose가 기본으로 붙는다 (deepagents/graph.py:792). 그 안에서 읽은 것은
#   세션에 남지 않으므로 prompts.py 규칙 5로 막는다. 아예 끄는 설정은
#   GeneralPurposeSubagentProfile(enabled=False)(graph.py:446 설명)인데, 프로필에 넣는 방법은 아직 확인 안 함.
# - 판정은 여기서 하지 않는다. judge 도구 = Rust 엔진.
