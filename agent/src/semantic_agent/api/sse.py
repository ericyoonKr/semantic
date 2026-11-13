import asyncio
import json
import logging
from collections.abc import AsyncIterator
from typing import Any

from langchain_core.messages import AIMessage, HumanMessage, ToolMessage
from langgraph.errors import GraphRecursionError

from semantic_agent.agent.budget import BudgetExceeded
from semantic_agent.agent.injection import INJECTION_KIND
from semantic_agent.api.answer_gate import SentenceGate, log_drop
from semantic_agent.api.sources import build_sources
from semantic_agent.api.emoji import strip_emoji
from semantic_agent.api.thought import ThoughtFilter
from semantic_agent.api.verify import verify_sources
from semantic_agent.ledger import LEDGER, Ledger
from semantic_agent.tools.client import REQUEST_ID, THREAD_ID, ToolClient, unwrap_material
from semantic_agent.funcall.readcheck import ReadCheck
from semantic_agent.tools.judge import needed_asks, verdict_text
from semantic_agent.turn import TURN, Turn

log = logging.getLogger(__name__)

LIMIT_STEPS = "최대 단계"
LIMIT_TIME = "질문 시간"
MAX_ROUNDS = 3
WRAP_UP_S = 90.0
WARNING_TEXT = {INJECTION_KIND: "읽은 자료 안에 지시처럼 보이는 문장이 있었습니다"}


UNREACHABLE = frozenset({"APIConnectionError", "APITimeoutError", "ConnectError", "ConnectTimeout", "OpenAIConnectionError"})


def model_unreachable(exc: BaseException) -> bool:
    seen: BaseException | None = exc
    for _ in range(6):
        if seen is None:
            return False
        if type(seen).__name__ in UNREACHABLE:
            return True
        seen = seen.__cause__ or seen.__context__
    return False


def event(name: str, data: dict[str, Any]) -> str:
    return f"event: {name}\ndata: {json.dumps(data, ensure_ascii=False)}\n\n"


MODEL_NODE = "model"


def _chunk_text(chunk: AIMessage) -> str:
    if isinstance(chunk.content, str):
        return chunk.content
    return "".join(b.get("text", "") for b in chunk.content if isinstance(b, dict) and b.get("type") == "text")


def _update_messages(update: Any) -> list[Any]:
    found: list[Any] = []
    if not isinstance(update, dict):
        return found
    for value in update.values():
        items = value if isinstance(value, list) else [value]
        for item in items:
            if isinstance(item, dict) and isinstance(item.get("messages"), list):
                found.extend(item["messages"])
    return found


def final_texts(update: Any) -> list[str]:
    if not isinstance(update, dict):
        return []
    out: list[str] = []
    for item in _update_messages({MODEL_NODE: update.get(MODEL_NODE)}):
        if isinstance(item, AIMessage) and not item.tool_calls:
            out.append(_chunk_text(item))
    return out


def _update_events(update: Any) -> list[str]:
    out: list[str] = []
    guard_nodes = [node for node in update if isinstance(node, str) and node.startswith("ScopeGuard")] if isinstance(update, dict) else []
    for message in _update_messages(update):
        if isinstance(message, AIMessage) and message.tool_calls:
            for call in message.tool_calls:
                out.append(event("Status", {"phase": "start", "tool": call["name"]}))
        elif isinstance(message, ToolMessage):
            content = message.content if isinstance(message.content, str) else str(message.content)
            out.append(event("Status", {"phase": "end", "tool": message.name or ""}))
            if unwrap_material(content).startswith("강등"):
                out.append(event("Warning", {"kind": "degraded", "message": "일부 자료를 찾지 못했습니다"}))
            if message.name == "judge" and not content.startswith("판정 불가"):
                out.append(event("VerdictReady", {"text": verdict_text(content)}))
                asks = needed_asks(content)
                if asks:
                    out.append(event("NeededFacts", {"asks": asks}))
        elif isinstance(message, AIMessage) and guard_nodes:
            out.append(event("Token", {"text": str(message.content)}))
    return out


def turn_events(turn: Turn) -> list[str]:
    warnings, audits = turn.drain()
    out = [event("Warning", {"kind": kind, "message": WARNING_TEXT.get(kind, kind)}) for kind in warnings]
    out += [event("Audit", item) for item in audits]
    return out


def usage_event(turn: Turn) -> str:
    return event("Usage", {"tokens": turn.tokens, "calls": turn.calls})


async def sources_event(gate: SentenceGate, client: ToolClient | None) -> list[str]:
    items = build_sources(gate.ledger, gate.cited)
    if not items:
        return []
    return [event("Sources", {"items": await verify_sources(client, gate.ledger, items)})]


def continue_round(turn: Turn, device: str, request_id: str, final: bool = False) -> bool:
    if turn.wrap_up:
        log.info("limit during wrap-up request_id=%s device=%s", request_id, device)
        return False
    turn.rounds += 1
    turn.round_tokens = turn.tokens
    turn.round_calls = turn.calls
    turn.wrap_up = final or turn.rounds >= MAX_ROUNDS
    log.info(
        "continue request_id=%s round=%d device=%s wrap_up=%s tokens=%d calls=%d",
        request_id,
        turn.rounds,
        device,
        turn.wrap_up,
        turn.tokens,
        turn.calls,
    )
    turn.audit("continue", {"round": turn.rounds, "device": device, "wrap_up": turn.wrap_up})
    return True


async def to_sse(
    agent: Any,
    question: str,
    cfg: dict[str, Any],
    timeout_s: float,
    client: ToolClient | None = None,
    ledger: Ledger | None = None,
) -> AsyncIterator[str]:
    request_id = str(cfg.get("configurable", {}).get("request_id", "-"))
    REQUEST_ID.set(request_id)
    ledger = ledger if ledger is not None else Ledger()
    ledger.search_failed = False
    LEDGER.set(ledger)
    turn = Turn()
    TURN.set(turn)
    THREAD_ID.set(str(cfg.get("configurable", {}).get("thread_id", "")))
    gate = SentenceGate(ledger, log_drop(request_id, turn), ReadCheck(client).verdicts)
    thought = ThoughtFilter()
    payload_in: Any = {"messages": [HumanMessage(content=question)]}
    budget_s = timeout_s
    while True:
        stream = agent.astream(payload_in, cfg, stream_mode=["updates"])
        try:
            async with asyncio.timeout(budget_s):
                async for _mode, payload in stream:
                    for item in _update_events(payload):
                        yield item
                    for answer in final_texts(payload):
                        text = strip_emoji(thought.push(answer))
                        for piece in await gate.push(text) if text else []:
                            yield event("Token", {"text": piece})
                    for item in turn_events(turn):
                        yield item
            break
        except (GraphRecursionError, BudgetExceeded) as exc:
            device = exc.device if isinstance(exc, BudgetExceeded) else LIMIT_STEPS
            if not continue_round(turn, device, request_id):
                break
            payload_in = None
        except TimeoutError:
            if not continue_round(turn, LIMIT_TIME, request_id, final=True):
                break
            payload_in = None
            budget_s = WRAP_UP_S
        except asyncio.CancelledError:
            raise
        except Exception as exc:
            log.error("ask failed type=%s request_id=%s", type(exc).__name__, request_id)
            for item in [*turn_events(turn), usage_event(turn)]:
                yield item
            kind = "model_unreachable" if model_unreachable(exc) else "failed"
            yield event("Error", {"message": "답을 만들지 못했습니다", "kind": kind, "request_id": request_id})
            return
        finally:
            await stream.aclose()
    for piece in [*await gate.push(strip_emoji(thought.flush())), *await gate.flush()]:
        yield event("Token", {"text": piece})
    for item in await sources_event(gate, client):
        yield item
    for item in [*turn_events(turn), usage_event(turn)]:
        yield item
    yield event("Done", {"request_id": request_id})
