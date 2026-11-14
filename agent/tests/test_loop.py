import asyncio
import json

import pytest
from conftest import answer, call, delegate, make_agent, run_config, tool_messages
from langchain_core.messages import AIMessage, ToolMessage

from semantic_agent.api.sse import EMPTY_ANSWER_TEXT, to_sse
from semantic_agent.config import Settings

FOUND = [{"claim": "가짜법 제1조는 목적 조항이다", "source": "가짜법 제1조 · 2000-01-01", "quote": "이 조문은 가짜 도구가 돌려주는 고정 문장이다."}]

QUESTIONS = [
    ("가짜법 제1조의 내용이 뭐야?", "search_laws", {"queries": ["가짜법 제1조"]}, "/search/laws"),
    ("이 쟁점에 관한 판례를 찾아 줘", "search_cases", {"queries": ["가짜 쟁점"], "k": 5}, "/search/cases"),
    ("어제 다쳤다. 국가배상 요건이 성립하나?", "judge", {"claim_type": "가짜청구", "node_id": "R1_가짜요건", "user_facts": ["어제 다쳤다"]}, None),
]


def _script(question: str, tool: str, args: dict) -> tuple[list[AIMessage], str]:
    final = AIMessage(content="가짜 답변 [가짜법 제1조]")
    if tool == "judge":
        return [call(tool, args, "c1"), final], "judge"
    return [delegate("researcher", question, "t1"), call(tool, args, "c1"), answer(FOUND), final], "task"


@pytest.mark.parametrize(("question", "tool", "args", "path"), QUESTIONS)
def test_question_picks_expected_fake_tool(question: str, tool: str, args: dict, path: str | None) -> None:
    async def scenario() -> None:
        script, main_tool = _script(question, tool, args)
        agent, model, client = await make_agent(script)
        cfg = run_config(Settings(internal_key="k", fake_tools=True).recursion_limit)
        events = [e async for e in to_sse(agent, question, cfg, timeout_s=60)]
        assert any(e.startswith("event: Status") and f'"tool": "{main_tool}"' in e for e in events)
        assert events[-1].startswith("event: Done")
        messages = (await agent.aget_state(cfg)).values["messages"]
        found = tool_messages(messages)
        assert [m.name for m in found] == [main_tool]
        assert found[0].content.startswith('<자료 출처="')
        assert messages[-1].text == "가짜 답변 [가짜법 제1조]"
        assert model.calls == len(script)
        if path is not None:
            assert client.calls == [("POST", path)]

    asyncio.run(scenario())


def test_streamed_answer_shows_checked_sources_only() -> None:
    async def scenario() -> None:
        quote = "이 조문 전문은 가짜 도구가 돌려주는 고정 문장이다."
        script = [
            delegate("researcher", "쟁점: 가짜법 제1조의 내용\n목적: 조회", "t1"),
            call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
            answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": quote}]),
            AIMessage(content="가짜법 제1조는 목적 조항입니다 [근거 1]. 민법 제750조도 적용됩니다.\n"),
        ]
        agent, _, client = await make_agent(script)
        cfg = run_config(Settings(internal_key="k", fake_tools=True).recursion_limit)
        events = [e async for e in to_sse(agent, "가짜법 제1조가 뭐야?", cfg, timeout_s=60, client=client)]
        tokens = "".join(
            json.loads(e.split("data: ", 1)[1])["text"] for e in events if e.startswith("event: Token")
        )
        assert "가짜법 제1조(시행 2000-01-01)" in tokens
        assert "[근거 1]" not in tokens
        assert "민법 제750조" not in tokens
        assert "뺐습니다" not in tokens

    asyncio.run(scenario())


def test_answer_with_every_sentence_dropped_says_so_instead_of_staying_blank() -> None:
    async def scenario() -> None:
        agent, _, client = await make_agent([AIMessage(content="민법 제750조가 적용됩니다.\n")])
        cfg = run_config(Settings(internal_key="k", fake_tools=True).recursion_limit)
        events = [e async for e in to_sse(agent, "질", cfg, timeout_s=60, client=client)]
        tokens = "".join(json.loads(e.split("data: ", 1)[1])["text"] for e in events if e.startswith("event: Token"))
        assert tokens.strip() == EMPTY_ANSWER_TEXT
        assert [e.split("\n", 1)[0] for e in events][-1] == "event: Done"

    asyncio.run(scenario())


def test_context_overflow_finishes_in_a_wrap_up_round_instead_of_an_error() -> None:
    from langchain_core.exceptions import ContextOverflowError

    async def scenario() -> None:
        script = [
            call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
            ContextOverflowError("문맥이 넘침"),
            AIMessage(content="가짜법 제1조는 목적 조항입니다.\n"),
        ]
        agent, model, client = await make_agent(script)
        cfg = run_config(Settings(internal_key="k", fake_tools=True).recursion_limit)
        events = [e async for e in to_sse(agent, "가짜법 제1조가 뭐야?", cfg, timeout_s=60, client=client)]
        names = [e.split("\n", 1)[0] for e in events]
        assert "event: Error" not in names
        assert names[-1] == "event: Done"
        tokens = "".join(json.loads(e.split("data: ", 1)[1])["text"] for e in events if e.startswith("event: Token"))
        assert tokens.strip()
        assert model.calls >= 2
    asyncio.run(scenario())


def test_overflow_shrinks_only_long_tool_results() -> None:
    from semantic_agent.agent.budget import OVERFLOW_CUT_NOTE, shrink_tool_messages

    long = ToolMessage(content="가" * 3000, tool_call_id="t1")
    short = ToolMessage(content="짧은 결과", tool_call_id="t2")
    shrunk = shrink_tool_messages([AIMessage(content="답"), long, short], 100)
    assert shrunk[1].content == "가" * 100 + OVERFLOW_CUT_NOTE
    assert shrunk[2].content == "짧은 결과"
    assert shrunk[0].content == "답"
    assert long.content == "가" * 3000
