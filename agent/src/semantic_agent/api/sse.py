import asyncio
import json
import logging
from collections.abc import AsyncIterator
from typing import Any

from langchain_core.messages import AIMessage, HumanMessage, ToolMessage
from langgraph.errors import GraphRecursionError

from semantic_agent.agent.authority import LEDGER, Ledger
from semantic_agent.agent.build import MAIN_AGENT
from semantic_agent.api.answer_gate import SentenceGate, log_drop
from semantic_agent.tools.client import REQUEST_ID, unwrap_material
from semantic_agent.tools.judge import needed_asks, verdict_text

log = logging.getLogger(__name__)

LIMIT_STEPS = "제한에 걸려 중단됨: 최대 단계"
LIMIT_TIME = "제한에 걸려 중단됨: 질문 시간"


def event(name: str, data: dict[str, Any]) -> str:
    return f"event: {name}\ndata: {json.dumps(data, ensure_ascii=False)}\n\n"


def _is_main_token(chunk: Any, metadata: dict[str, Any]) -> bool:
    return (
        isinstance(chunk, AIMessage)
        and metadata.get("lc_agent_name") == MAIN_AGENT
        and metadata.get("langgraph_node") == "model"
        and metadata.get("lc_source") != "summarization"
        and not metadata.get("lc_internal_call")
    )


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


async def to_sse(agent: Any, question: str, cfg: dict[str, Any], timeout_s: float) -> AsyncIterator[str]:
    request_id = str(cfg.get("configurable", {}).get("request_id", "-"))
    REQUEST_ID.set(request_id)
    ledger = Ledger()
    LEDGER.set(ledger)
    gate = SentenceGate(ledger, log_drop(request_id))
    stream = agent.astream(
        {"messages": [HumanMessage(content=question)]},
        cfg,
        stream_mode=["messages", "updates"],
    )
    try:
        async with asyncio.timeout(timeout_s):
            async for mode, payload in stream:
                if mode == "messages":
                    chunk, metadata = payload
                    if _is_main_token(chunk, metadata):
                        text = _chunk_text(chunk)
                        for piece in gate.push(text) if text else []:
                            yield event("Token", {"text": piece})
                elif mode == "updates":
                    for item in _update_events(payload):
                        yield item
        for piece in gate.flush():
            yield event("Token", {"text": piece})
    except GraphRecursionError:
        for piece in gate.flush():
            yield event("Token", {"text": piece})
        yield event("Token", {"text": f"\n\n{LIMIT_STEPS}"})
        yield event("Warning", {"kind": "limit", "message": LIMIT_STEPS})
    except TimeoutError:
        for piece in gate.flush():
            yield event("Token", {"text": piece})
        yield event("Token", {"text": f"\n\n{LIMIT_TIME}"})
        yield event("Warning", {"kind": "limit", "message": LIMIT_TIME})
    except asyncio.CancelledError:
        raise
    except Exception as exc:
        log.error("ask failed type=%s request_id=%s", type(exc).__name__, request_id)
        yield event("Error", {"message": "답을 만들지 못했습니다", "request_id": request_id})
        return
    finally:
        await stream.aclose()
    yield event("Done", {"request_id": request_id})
