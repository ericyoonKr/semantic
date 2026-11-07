import asyncio
import json
import logging
import time
from collections.abc import Awaitable, Callable, Sequence
from dataclasses import dataclass
from typing import Any

from langchain.agents.middleware import AgentMiddleware
from langchain_core.messages import ToolMessage
from langgraph.prebuilt.tool_node import ToolCallRequest
from langgraph.types import Command

from semantic_agent.agent.prompts import RETRY_NOTE
from semantic_agent.agent.subagents import LIMIT_MARK, SUB_DEADLINE, SUB_DEADLINE_S
from semantic_agent.tools.client import material

log = logging.getLogger(__name__)

TASK = "task"
SUB_CONCURRENCY = 2
RESEARCH_FAIL = "자료 조사 실패"


@dataclass(frozen=True)
class Finding:
    claim: str
    source: str
    quote: str


@dataclass(frozen=True)
class Findings:
    found: list[Finding]
    not_found: list[str]
    dropped: int
    limit: str | None


def _json_object(text: str) -> Any:
    start, end = text.find("{"), text.rfind("}")
    if start < 0 or end <= start:
        return None
    try:
        return json.loads(text[start : end + 1])
    except ValueError:
        return None


def _clean(value: Any) -> str:
    return value.strip() if isinstance(value, str) else ""


def parse_findings(text: str) -> Findings | None:
    data = _json_object(text)
    if not isinstance(data, dict):
        return None
    found, not_found = data.get("found"), data.get("not_found")
    if not isinstance(found, list) or not isinstance(not_found, list):
        return None
    kept: list[Finding] = []
    dropped = 0
    for item in found:
        entry = item if isinstance(item, dict) else {}
        claim, source = _clean(entry.get("claim")), _clean(entry.get("source"))
        if not claim or not source:
            dropped += 1
            continue
        kept.append(Finding(claim, source, _clean(entry.get("quote"))))
    misses = [m for m in (_clean(x) for x in not_found) if m]
    limit = next((s for s in (line.strip() for line in text.splitlines()) if s.startswith(LIMIT_MARK)), None)
    return Findings(kept, misses, dropped, limit)


def render_findings(name: str, findings: Findings) -> str:
    lines = ["찾은 것:"]
    for item in findings.found:
        lines += [f"- 주장: {item.claim}", f"  출처: {item.source}", f"  인용: {item.quote}"]
    if not findings.found:
        lines.append("- 없음")
    lines.append("찾지 못한 것:")
    lines += [f"- {miss}" for miss in findings.not_found] or ["- 없음"]
    if findings.limit:
        lines.append(findings.limit)
    return material(f"하위 에이전트 {name}", "\n".join(lines))


def _result_text(result: ToolMessage | Command) -> str:
    if isinstance(result, ToolMessage):
        message: Any = result
    else:
        update = result.update if isinstance(result.update, dict) else {}
        messages = update.get("messages") or [None]
        message = messages[-1]
    content = getattr(message, "content", "")
    return content if isinstance(content, str) else str(content)


def _with_text(result: ToolMessage | Command, call_id: str, text: str) -> ToolMessage | Command:
    message = ToolMessage(content=text, tool_call_id=call_id, name=TASK)
    if isinstance(result, Command) and isinstance(result.update, dict):
        return Command(update={**result.update, "messages": [message]})
    return message


def _question_key(request: ToolCallRequest) -> str:
    config = getattr(request.runtime, "config", None) or {}
    conf = config.get("configurable", {}) if isinstance(config, dict) else {}
    return f"{conf.get('thread_id', '')}/{conf.get('request_id', '')}"


@dataclass
class _Slot:
    semaphore: asyncio.Semaphore
    users: int = 0


class SubagentGate(AgentMiddleware):
    def __init__(
        self,
        names: Sequence[str],
        limit: int = SUB_CONCURRENCY,
        deadline_s: float = SUB_DEADLINE_S,
    ) -> None:
        super().__init__()
        self._names = frozenset(names)
        self._limit = limit
        self._deadline_s = deadline_s
        self._slots: dict[str, _Slot] = {}

    async def awrap_tool_call(
        self,
        request: ToolCallRequest,
        handler: Callable[[ToolCallRequest], Awaitable[ToolMessage | Command]],
    ) -> ToolMessage | Command:
        call = request.tool_call
        if call["name"] != TASK or (call.get("args") or {}).get("subagent_type") not in self._names:
            return await handler(request)
        key = _question_key(request)
        slot = self._slots.get(key)
        if slot is None:
            slot = _Slot(asyncio.Semaphore(self._limit))
            self._slots[key] = slot
        slot.users += 1
        try:
            async with slot.semaphore:
                return await self._delegate(request, handler)
        finally:
            slot.users -= 1
            if slot.users == 0:
                self._slots.pop(key, None)

    async def _run_once(
        self,
        request: ToolCallRequest,
        handler: Callable[[ToolCallRequest], Awaitable[ToolMessage | Command]],
    ) -> ToolMessage | Command | None:
        token = SUB_DEADLINE.set(time.monotonic() + self._deadline_s)
        try:
            async with asyncio.timeout(self._deadline_s):
                return await handler(request)
        except TimeoutError:
            return None
        finally:
            SUB_DEADLINE.reset(token)

    async def _delegate(
        self,
        request: ToolCallRequest,
        handler: Callable[[ToolCallRequest], Awaitable[ToolMessage | Command]],
    ) -> ToolMessage | Command:
        call = request.tool_call
        args = dict(call.get("args") or {})
        name, call_id = str(args.get("subagent_type")), call.get("id") or ""
        attempt = request
        for round_no in (1, 2):
            result = await self._run_once(attempt, handler)
            if result is None:
                log.info("subagent timed out name=%s round=%d", name, round_no)
                limit = f"{LIMIT_MARK}: 시간 상한 {int(self._deadline_s)}초"
                return ToolMessage(content=f"{RESEARCH_FAIL}: {name} {limit}", tool_call_id=call_id, name=TASK)
            findings = parse_findings(_result_text(result))
            if findings is not None:
                if findings.dropped:
                    log.info("subagent lines without source dropped name=%s count=%d", name, findings.dropped)
                return _with_text(result, call_id, render_findings(name, findings))
            log.info("subagent result malformed name=%s round=%d", name, round_no)
            retry_args = {**args, "description": f"{args.get('description', '')}\n\n{RETRY_NOTE}"}
            attempt = request.override(tool_call={**call, "args": retry_args})
        return ToolMessage(
            content=f"{RESEARCH_FAIL}: {name}의 결과가 정해진 형식에 맞지 않음", tool_call_id=call_id, name=TASK
        )
