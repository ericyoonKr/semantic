import asyncio

from semantic_agent.api.runlog import RunLog
from semantic_agent.api.sessions import SessionRegistry


def test_follow_replays_from_start_and_waits_for_new_items() -> None:
    async def go() -> list[str]:
        log = RunLog("질문")
        log.add("a")
        got: list[str] = []

        async def reader() -> None:
            async for item in log.follow():
                got.append(item)

        task = asyncio.create_task(reader())
        await asyncio.sleep(0)
        log.add("b")
        await asyncio.sleep(0)
        log.finish()
        await asyncio.wait_for(task, 1)
        return got

    assert asyncio.run(go()) == ["a", "b"]


def test_live_returns_only_unfinished_runs() -> None:
    registry = SessionRegistry()
    assert registry.live("s") is None
    slot = registry.get("s")
    slot.log = RunLog("질문")
    assert registry.live("s") is slot.log
    slot.log.finish()
    assert registry.live("s") is None
