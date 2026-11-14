import json
from typing import Any

from langchain_core.messages import AIMessage
from langgraph.store.base import BaseStore

from semantic_agent.ledger import Ledger

ANSWER_LIMIT = 1000


def answers_namespace(user_id: str, session_id: str) -> tuple[str, ...]:
    return ("answers", user_id, session_id)


def _event(item: str) -> tuple[str, Any]:
    name, data = "", None
    for line in item.splitlines():
        if line.startswith("event: "):
            name = line[len("event: ") :]
        elif line.startswith("data: "):
            try:
                data = json.loads(line[len("data: ") :])
            except json.JSONDecodeError:
                data = None
    return name, data


def rendered(items: list[str]) -> tuple[str, list[dict[str, Any]]]:
    text: list[str] = []
    sources: list[dict[str, Any]] = []
    for item in items:
        name, data = _event(item)
        if not isinstance(data, dict):
            continue
        if name == "Token" and isinstance(data.get("text"), str):
            text.append(data["text"])
        elif name == "Sources" and isinstance(data.get("items"), list):
            sources = [s for s in data["items"] if isinstance(s, dict)]
    return "".join(text), sources


def context_of(items: list[str]) -> dict[str, int] | None:
    for item in items:
        name, data = _event(item)
        if name != "Usage" or not isinstance(data, dict):
            continue
        used, limit, summary_at = data.get("context"), data.get("context_limit"), data.get("summary_at")
        if isinstance(used, int) and isinstance(limit, int) and used > 0 and limit > 0:
            return {"used": used, "limit": limit, "summary_at": summary_at if isinstance(summary_at, int) else 0}
    return None


def last_answer_id(messages: list[Any]) -> str | None:
    for message in reversed(messages):
        if isinstance(message, AIMessage) and not message.tool_calls and message.text:
            return message.id
    return None


async def save_answer(store: BaseStore, user_id: str, session_id: str, message_id: str, items: list[str]) -> bool:
    text, sources = rendered(items)
    if not text.strip():
        return False
    value: dict[str, Any] = {"text": text, "sources": sources}
    context = context_of(items)
    if context is not None:
        value["context"] = context
    await store.aput(answers_namespace(user_id, session_id), message_id, value)
    return True


async def saved_answers(store: BaseStore, user_id: str, session_id: str) -> dict[str, dict[str, Any]]:
    found = await store.asearch(answers_namespace(user_id, session_id), limit=ANSWER_LIMIT)
    return {item.key: item.value for item in found}


async def delete_answer(store: BaseStore, user_id: str, session_id: str, message_id: str) -> None:
    await store.adelete(answers_namespace(user_id, session_id), message_id)


async def delete_answers(store: BaseStore, user_id: str, session_id: str) -> int:
    namespace = answers_namespace(user_id, session_id)
    found = await store.asearch(namespace, limit=ANSWER_LIMIT)
    for item in found:
        await store.adelete(namespace, item.key)
    return len(found)


LEDGER_KEY = "ledger"


def ledger_namespace(user_id: str, session_id: str) -> tuple[str, ...]:
    return ("ledger", user_id, session_id)


async def save_ledger(store: BaseStore, user_id: str, session_id: str, ledger: Ledger) -> None:
    await store.aput(ledger_namespace(user_id, session_id), LEDGER_KEY, ledger.dump())


async def load_ledger(store: BaseStore, user_id: str, session_id: str) -> Ledger:
    found = await store.aget(ledger_namespace(user_id, session_id), LEDGER_KEY)
    return Ledger.load(found.value) if found is not None else Ledger()


async def delete_ledger(store: BaseStore, user_id: str, session_id: str) -> None:
    await store.adelete(ledger_namespace(user_id, session_id), LEDGER_KEY)
