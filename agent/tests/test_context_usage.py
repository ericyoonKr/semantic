import json

from langchain_core.messages import AIMessage, HumanMessage

from semantic_agent.api.answers import context_of
from semantic_agent.api.app import history_items
from semantic_agent.api.sse import usage_event
from semantic_agent.turn import Turn


def test_usage_event_carries_the_context_size_and_limits() -> None:
    turn = Turn(tokens=5000, calls=3, context_tokens=22204)
    item = usage_event(turn, 32768)
    data = json.loads(item.split("data: ", 1)[1])
    assert data == {"tokens": 5000, "calls": 3, "context": 22204, "context_limit": 32768, "summary_at": 20000}
    assert context_of([item]) == {"used": 22204, "limit": 32768, "summary_at": 20000}
    assert context_of([usage_event(Turn(), 32768)]) is None
    assert context_of(["event: Done\ndata: {}\n\n"]) is None


def test_history_items_carry_the_saved_context() -> None:
    messages = [HumanMessage(content="질문", id="h1"), AIMessage(content="답", id="a1")]
    context = {"used": 22204, "limit": 32768, "summary_at": 20000}
    with_context = history_items(messages, {"a1": {"text": "답", "sources": [], "context": context}})
    assert with_context[1]["context"] == context
    plain = history_items(messages, {"a1": {"text": "답", "sources": []}})
    assert "context" not in plain[1]
