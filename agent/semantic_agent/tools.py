# 에이전트 도구 — ① Rust 내부 도구 API 호출 (PROJECT.md §5 ①·③·⑤·⑦·⑨)
# 도구가 돌려준 문자열이 곧 「에이전트가 읽은 내용」으로 세션에 저장된다 (memory.py).
#
# 담을 것 (langchain_core.tools.tool, 전부 async, 공용 httpx.AsyncClient 하나):
# - search_laws(query, k=8)                → POST /search/laws                 법령·조문 top-k
# - search_cases(query, k=8)               → POST /search/cases                판례 top-k
# - search_upload(query, k=8)              → POST /sessions/{sid}/upload_search 업로드 문서 top-k
# - read_clauses(file_id, clause_ids)      → GET  /sessions/{sid}/files/{file_id}/clauses
# - judge(claim_type, node_id, assumptions) → POST /sessions/{sid}/judge
#     요건 판단형 질문 전용 (옛 ⑨ 라우팅이 이 도구 선택으로 바뀌었다).
#     Rust가 Z3로 판정하고 ⑩ 템플릿으로 만든 판정 문장을 돌려준다.
#     도구 설명(docstring)에 노드 카탈로그(GET /catalog — claim_type별 node_id·description)를 넣는다.
#     카탈로그는 부팅 때 한 번 받는다. 예시:
#       "이게 처분에 해당하나?"    → judge("취소소송", "S1_처분성", [])       (서브트리만 판정)
#       "제소기간 지났나?"         → judge("취소소송", "S3_제소기간", [])
#       "처분성은 인정된다 치고 …" → assumptions=[("S1_처분성", true)]
#       "제5조가 무슨 뜻이야?"     → judge가 아니라 search_upload + search_laws
#
# - sid(세션 ID)는 모델이 채우지 않는다. 도구 인자 runtime: ToolRuntime에서
#   runtime.config["configurable"]["thread_id"]로 읽는다 (모델에 보이는 인자 목록에 안 나온다 —
#   2026-09-29 확인). 모델이 다른 세션의 문서를 부르는 길을 막는다.
#
# 결과 문자열 규칙:
# - 항목마다 출처를 붙인다: 법령은 법령명·조문번호·시행일, 판례는 사건번호·선고일,
#   업로드는 file_id·clause_id·페이지. 답변의 인용이 이 문자열에서 나온다.
# - Rust가 degraded(FTS 단독 강등 등)를 알리면 결과 첫 줄에 적는다 (§11).
# - HTTP 오류·timeout은 예외로 올리지 않고 「검색 실패: 사유」 문자열로 돌려준다 —
#   에이전트가 실패를 알고 답변에 밝히게 (§2 원칙 2). judge 실패는 「판정 불가: 사유」.
