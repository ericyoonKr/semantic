import asyncio
from collections.abc import AsyncIterator
from dataclasses import dataclass, field


@dataclass
class RunLog:
    question: str
    items: list[str] = field(default_factory=list)
    done: bool = False
    changed: asyncio.Event = field(default_factory=asyncio.Event)

    def add(self, item: str) -> None:
        self.items.append(item)
        self.changed.set()

    def finish(self) -> None:
        self.done = True
        self.changed.set()

    async def follow(self, start: int = 0) -> AsyncIterator[str]:
        at = start
        while True:
            while at < len(self.items):
                yield self.items[at]
                at += 1
            if self.done:
                return
            self.changed.clear()
            if at < len(self.items) or self.done:
                continue
            await self.changed.wait()
