# 세션별 실행 관리 — 같은 세션은 한 번에 하나, 업로드가 진행 중인 질문보다 우선 (PROJECT.md §10)
# app.py(엔드포인트)와 나눈 이유: 동시성 규칙은 엔드포인트가 늘어나도 한 곳에서 지켜야 한다.
#
# 담을 것:
# - class SessionRegistry: dict[session_id, SessionSlot] (없으면 만든다)
# - class SessionSlot: lock: asyncio.Lock, running_ask: asyncio.Task | None
# - async def run_ask(slot, coro) : 락을 잡고 질문 실행 태스크를 running_ask로 기록한 뒤 실행
# - async def run_upload(slot, coro): running_ask가 있으면 cancel() → 끝나기를 기다린 뒤 락을 잡고 업로드
#
# 규칙:
# - 한 thread_id에 실행은 하나만 (memory.py 규칙) — 같은 체크포인트에서 갈라진 두 실행 중
#   한쪽 기록이 최신 상태에서 빠질 수 있다. 세션 사이는 병렬.
# - 업로드는 진행 중인 ask보다 우선한다 (§10 문서 수정판 우선).
#   Rust 쪽 판정은 session_actor가 업로드 도착 때 취소한다.
#   취소로 답 없이 남은 도구 호출은 다음 실행 때 PatchToolCallsMiddleware(기본으로 켜짐)가 메운다.
# - 오래 안 쓰인 SessionSlot은 지운다 (메모리만 — 대화 기록은 체크포인트에 남아 있다).
