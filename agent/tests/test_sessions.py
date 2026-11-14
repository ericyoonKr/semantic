import asyncio

from semantic_agent.api.sessions import SessionRegistry, run_ask


def test_cancel_stops_the_running_ask_and_frees_the_slot() -> None:
    async def scenario() -> None:
        registry = SessionRegistry()
        slot = registry.get("s1")
        started = asyncio.Event()

        async def work() -> None:
            started.set()
            await asyncio.sleep(60)

        outer = asyncio.create_task(run_ask(slot, work()))
        await started.wait()
        assert slot.running_ask is not None and slot.lock.locked()
        await registry.cancel("s1")
        await asyncio.gather(outer, return_exceptions=True)
        assert outer.cancelled()
        assert slot.running_ask is None and not slot.lock.locked()
        await registry.cancel("s1")
        await registry.cancel("missing")

    asyncio.run(scenario())
