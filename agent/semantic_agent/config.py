# 설정 — 환경변수 / .env (pydantic-settings)
#
# 담을 것:
# - class Settings(BaseSettings):
#                              # 다른 데이터베이스로 둔다 — 코퍼스는 읽기 전용 (§3.1). 위치는 §12 결정 4.
#     pg_pool_max: int = 10    # 체크포인트 연결 풀 크기. 동시에 도는 세션 수에 맞춰 정한다.
#     rust_api_url: str = "http://127.0.0.1:8080"   # ① 내부 도구 API
#     rust_timeout_s: float = 120  # judge는 리프 판정 fan-out이라 검색보다 오래 걸린다 (§10)
#     llm_base_url: str; llm_api_key: str; llm_model: str
#                              # OpenAI 호환 엔드포인트 — 외부 API·로컬 서버(§12 결정 2) 어느 쪽이든
#                              # base_url만 바꾼다. 그래서 langchain-openai 하나만 둔다.
