import asyncio
import json
import logging
import time
from collections.abc import Awaitable, Callable, Mapping, Sequence
from dataclasses import dataclass, replace
from typing import Any

from langchain.agents.middleware import AgentMiddleware
from langchain_core.messages import ToolMessage
from langgraph.prebuilt.tool_node import ToolCallRequest
from langgraph.types import Command

from semantic_agent.agent.citations import Finding, Findings, check_research, judge_relevance
from semantic_agent.agent.guards import result_text
from semantic_agent.agent.intent import IntentScope
from semantic_agent.agent.issues import flag, parse_issues, render_issues
from semantic_agent.agent.prompts import ADVERSE_NOTE, RETRY_NOTE
from semantic_agent.agent.prompts_research import INTENT_NOTE
from semantic_agent.agent.subagents import LIMIT_MARK, RESEARCHER, SUB_DEADLINE, SUB_DEADLINE_S
from semantic_agent.funcall.readcheck import ReadCheck
from semantic_agent.ledger import Ledger, current_ledger, normalize
from semantic_agent.tools.client import ToolClient, material
from semantic_agent.turn import current_turn

log = logging.getLogger(__name__)

TASK = "task"
SUB_CONCURRENCY = 2
RESEARCH_FAIL = "자료 조사 실패"
ADVERSE_RETRIES = 2
ISSUE_HEAD = "쟁점:"
PURPOSE_HEAD = "목적:"
JUDGE_PURPOSE = "판단"
STANCES = frozenset({"support", "adverse", "neutral"})


@dataclass(frozen=True)
class Brief:
    issue: str
    purpose: str


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
        stance = _clean(entry.get("stance"))
        kept.append(
            Finding(
                claim,
                source,
                _clean(entry.get("quote")),
                stance if stance in STANCES else "neutral",
                annex_used=flag(entry.get("annex_used")),
                old_law_case=flag(entry.get("old_law_case")),
            )
        )
    misses = [m for m in (_clean(x) for x in not_found) if m]
    limit = next((s for s in (line.strip() for line in text.splitlines()) if s.startswith(LIMIT_MARK)), None)
    reverse = _clean(data.get("reverse_query")) or None
    return Findings(kept, misses, dropped, limit, reverse, parse_issues(data.get("issues")))


def _query_key(text: str) -> str:
    return "".join(ch for ch in text if ch.isalnum())


def reverse_ok(findings: Findings, queries: Sequence[str]) -> bool:
    if any(item.stance == "adverse" for item in findings.found):
        return True
    if findings.reverse_query is None:
        return False
    wanted = _query_key(findings.reverse_query)
    return bool(wanted) and any(_query_key(q) == wanted for q in queries)


def render_findings(name: str, findings: Findings) -> str:
    lines = ["찾은 것:"]
    for item in findings.found:
        lines += [f"- 주장: {item.claim}", f"  출처: {item.source}", f"  인용: {item.quote}"]
        if item.stance != "neutral":
            lines.append(f"  방향: {item.stance}")
        if item.annex_used:
            lines.append("  별표 계산값 사용")
        if item.old_law_case:
            lines.append("  구법 판례")
    if not findings.found:
        lines.append("- 없음")
    lines.append("찾지 못한 것:")
    lines += [f"- {miss}" for miss in findings.not_found] or ["- 없음"]
    lines += render_issues(findings.issues)
    if findings.reverse_query:
        lines.append(f"반대 방향 검색어: {findings.reverse_query}")
    if findings.limit:
        lines.append(findings.limit)
    return material(f"하위 에이전트 {name}", "\n".join(lines))


def read_brief(description: str) -> Brief:
    issue, purpose = "", ""
    for line in description.splitlines():
        text = line.strip()
        if text.startswith(ISSUE_HEAD) and not issue:
            issue = text[len(ISSUE_HEAD) :].strip()
        elif text.startswith(PURPOSE_HEAD) and not purpose:
            purpose = text[len(PURPOSE_HEAD) :].strip()
    return Brief(issue or description.strip(), JUDGE_PURPOSE if purpose.startswith(JUDGE_PURPOSE) else "조회")


def route_hint(issue: str, routes: Mapping[str, Sequence[str]]) -> str:
    units: list[str] = []
    for claim_type, laws in routes.items():
        if claim_type and claim_type in issue:
            units += [u for u in laws if u not in units]
    return f"먼저 읽을 조문: {', '.join(units)}" if units else ""


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
        routes: Mapping[str, Sequence[str]] | None = None,
        scope: IntentScope | None = None,
        client: ToolClient | None = None,
    ) -> None:
        super().__init__()
        self._judge = ReadCheck(client).judge
        self._names = frozenset(names)
        self._limit = limit
        self._deadline_s = deadline_s
        self._routes = dict(routes or {})
        self._scope = scope
        self._slots: dict[str, _Slot] = {}

    async def awrap_tool_call(
        self,
        request: ToolCallRequest,
        handler: Callable[[ToolCallRequest], Awaitable[ToolMessage | Command]],
    ) -> ToolMessage | Command:
        call = request.tool_call
        args = call.get("args") or {}
        name = args.get("subagent_type")
        if call["name"] != TASK or name not in self._names:
            return await handler(request)
        ledger = current_ledger()
        brief = read_brief(str(args.get("description", "")))
        seen_key = f"{name}\x00{normalize(brief.issue)}"
        seen = ledger.delegations.get(seen_key)
        if seen is not None:
            log.info("subagent issue repeated name=%s", name)
            return ToolMessage(content=seen, tool_call_id=call.get("id") or "", name=TASK)
        key = _question_key(request)
        slot = self._slots.get(key)
        if slot is None:
            slot = _Slot(asyncio.Semaphore(self._limit))
            self._slots[key] = slot
        slot.users += 1
        try:
            async with slot.semaphore:
                result = await self._delegate(request, handler, ledger, brief)
        finally:
            slot.users -= 1
            if slot.users == 0:
                self._slots.pop(key, None)
        text = result_text(result)
        if not text.startswith(RESEARCH_FAIL):
            ledger.delegations[seen_key] = text
        return result

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

    def _first_request(self, request: ToolCallRequest, name: str, brief: Brief) -> ToolCallRequest:
        hint = route_hint(brief.issue, self._routes) if name == RESEARCHER else ""
        if not hint:
            return request
        call = request.tool_call
        args = dict(call.get("args") or {})
        args["description"] = f"{args.get('description', '')}\n{hint}"
        return request.override(tool_call={**call, "args": args})

    async def _delegate(
        self,
        request: ToolCallRequest,
        handler: Callable[[ToolCallRequest], Awaitable[ToolMessage | Command]],
        ledger: Ledger,
        brief: Brief,
    ) -> ToolMessage | Command:
        call = request.tool_call
        args = dict(call.get("args") or {})
        name, call_id = str(args.get("subagent_type")), call.get("id") or ""
        attempt = self._first_request(request, name, brief)
        malformed = 0
        adverse = 0
        followed = self._scope is None or name != RESEARCHER
        start = len(ledger.queries)
        while True:
            result = await self._run_once(attempt, handler)
            if result is None:
                log.info("subagent timed out name=%s", name)
                limit = f"{LIMIT_MARK}: 시간 상한 {int(self._deadline_s)}초"
                return ToolMessage(content=f"{RESEARCH_FAIL}: {name} {limit}", tool_call_id=call_id, name=TASK)
            findings = parse_findings(result_text(result))
            note = ""
            if findings is None:
                malformed += 1
                log.info("subagent result malformed name=%s round=%d", name, malformed)
                if malformed >= 2:
                    return ToolMessage(
                        content=f"{RESEARCH_FAIL}: {name}의 결과가 정해진 형식에 맞지 않음",
                        tool_call_id=call_id,
                        name=TASK,
                    )
                note = RETRY_NOTE
            else:
                if name == RESEARCHER:
                    findings = await judge_relevance(check_research(findings, ledger), ledger, self._judge, brief.issue)
                    if brief.purpose == JUDGE_PURPOSE and not reverse_ok(findings, ledger.queries[start:]):
                        adverse += 1
                        log.info(
                            "subagent reverse query unverified name=%s round=%d reverse=%r called=%r",
                            name,
                            adverse,
                            findings.reverse_query,
                            ledger.queries[start:][-6:],
                        )
                        if adverse <= ADVERSE_RETRIES:
                            note = ADVERSE_NOTE
                        else:
                            actual = ledger.queries[start:]
                            findings = replace(findings, reverse_query=actual[-1] if actual else None)
                if not note and not followed and self._scope is not None:
                    followed = True
                    hint = await self._scope.hint(brief.issue, findings.issues, ledger)
                    note = f"{hint}\n\n{INTENT_NOTE}" if hint else ""
                if not note:
                    if findings.dropped:
                        log.info("subagent lines dropped name=%s count=%d", name, findings.dropped)
                        current_turn().block("sub_result", name=name, dropped=findings.dropped)
                    if findings.irrelevant:
                        log.info("subagent lines off the issue name=%s count=%d", name, findings.irrelevant)
                        current_turn().block("sub_result", name=name, irrelevant=findings.irrelevant)
                    return _with_text(result, call_id, render_findings(name, findings))
            current = dict(attempt.tool_call.get("args") or {})
            current["description"] = f"{current.get('description', '')}\n\n{note}"
            attempt = attempt.override(tool_call={**attempt.tool_call, "args": current})
