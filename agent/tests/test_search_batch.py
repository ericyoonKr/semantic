import asyncio
import time

from semantic_agent.ledger import LEDGER, Ledger
from semantic_agent.tools.search import run_queries


def test_queries_run_together_and_keep_their_order() -> None:
    async def scenario() -> tuple[str, float, list[str]]:
        ledger = Ledger()
        LEDGER.set(ledger)

        async def one(q: str) -> str:
            await asyncio.sleep(0.2)
            return f"결과 {q}"

        start = time.perf_counter()
        text = await run_queries(["가", "나", " 가 ", "다"], one)
        return text, time.perf_counter() - start, list(ledger.queries)

    text, took, recorded = asyncio.run(scenario())
    assert took < 0.4
    assert text.index("### 검색어 1: 가") < text.index("### 검색어 2: 나") < text.index("### 검색어 3: 다")
    assert text.count("결과 가") == 1
    assert recorded == ["가", "나", "다"]


def test_single_query_keeps_plain_result() -> None:
    async def one(q: str) -> str:
        return f"결과 {q}"

    token = LEDGER.set(Ledger())
    try:
        assert asyncio.run(run_queries(["가"], one)) == "결과 가"
    finally:
        LEDGER.reset(token)


def test_case_search_stops_after_case_data_is_missing() -> None:
    from semantic_agent.tools.search import case_data_gone, case_failed

    token = LEDGER.set(Ledger())
    try:
        assert case_data_gone() is None
        assert case_failed("검색 실패: 시간 초과 (요청 ID r1)") == "검색 실패: 시간 초과 (요청 ID r1)"
        assert case_data_gone() is None
        said = "검색 실패: 판례 자료가 아직 법령·판례 DB에 들어 있지 않습니다"
        assert case_failed(f"{said} (요청 ID r2)") == f"{said} (요청 ID r2)"
        assert case_data_gone() == said
    finally:
        LEDGER.reset(token)
