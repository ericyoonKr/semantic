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
