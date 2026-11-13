import asyncio
import json

from conftest import answer, call, make_agent, run_config
from langchain_core.messages import AIMessage, ToolMessage

from semantic_agent.agent.research import ResearchOptions
from semantic_agent.api.sse import to_sse

QUOTE = "이 조문 전문은 가짜 도구가 돌려주는 고정 문장이다."


def _tokens(events: list[str]) -> str:
    return "".join(json.loads(e.split("data: ", 1)[1])["text"] for e in events if e.startswith("event: Token"))


async def _ask(script: list[AIMessage], question: str) -> tuple[list[str], object, dict]:
    agent, model, client = await make_agent(script, research=ResearchOptions(first=True))
    cfg = run_config()
    events = [e async for e in to_sse(agent, question, cfg, timeout_s=60, client=client)]
    return events, model, cfg


def test_research_runs_before_the_model_answers() -> None:
    script = [
        call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
        AIMessage(content="가짜법 제1조는 목적 조항입니다 [근거 1].\n"),
    ]
    events, model, cfg = asyncio.run(_ask(script, "가짜법 제1조가 뭐야?"))
    assert any(e.startswith("event: Status") and '"tool": "task"' in e for e in events)
    first_main = model.seen[-1]
    assert any(isinstance(m, ToolMessage) and m.name == "task" for m in first_main)
    assert "가짜법 제1조(시행 2000-01-01)" in _tokens(events)
    assert events[-1].startswith("event: Done")


def test_empty_final_answer_is_written_again() -> None:
    script = [
        answer([], ["찾지 못함"]),
        AIMessage(content="<|channel>thought 정리 중<channel|>"),
        AIMessage(content="조사한 자료로는 확인되지 않습니다.\n"),
    ]
    events, model, _ = asyncio.run(_ask(script, "아무 법이나 알려줘"))
    assert _tokens(events).strip() == "조사한 자료로는 확인되지 않습니다."
    assert model.calls == 3
