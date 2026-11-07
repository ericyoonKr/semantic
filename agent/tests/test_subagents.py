import asyncio
from typing import Any

import pytest
from conftest import answer, call, delegate, make_agent, run_config, tool_messages
from langchain_core.language_models import BaseChatModel
from langchain_core.messages import AIMessage, HumanMessage, SystemMessage
from langchain_core.outputs import ChatGeneration, ChatResult

from semantic_agent.agent import build
from semantic_agent.agent.delegation import RESEARCH_FAIL, SubagentGate
from semantic_agent.agent.prompts import RESEARCHER_PROMPT, RETRY_NOTE

LAW_QUOTE = "이 조문은 가짜 도구가 돌려주는 고정 문장이다."
KEPT = {"claim": "가짜법 제1조는 목적 조항이다", "source": "가짜법 제1조 · 2000-01-01", "quote": LAW_QUOTE}
UNSOURCED = {"claim": "출처 없는 가짜 주장", "source": "", "quote": LAW_QUOTE}
RESEARCHER_TOOLS = {"search_laws", "search_cases", "read_law", "read_case", "case_citations"}


async def _ask(script: list[AIMessage]) -> tuple[list[Any], Any, Any]:
    agent, model, client = await make_agent(script)
    result = await agent.ainvoke({"messages": [HumanMessage(content="가짜법 제1조가 뭐야?")]}, run_config())
    return result["messages"], model, client


def test_main_delegates_to_researcher_and_answers_from_result() -> None:
    script = [
        delegate("researcher", "가짜법 제1조의 내용을 찾아 출처와 함께 돌려줘", "t1"),
        call("search_laws", {"query": "가짜법 제1조"}, "c1"),
        answer([KEPT], ["가짜법 제2조"]),
        AIMessage(content="가짜법 제1조는 목적 조항입니다 [가짜법 제1조 · 2000-01-01]"),
    ]
    messages, model, client = asyncio.run(_ask(script))
    [result] = tool_messages(messages)
    assert result.name == "task"
    assert result.content.startswith('<자료 출처="하위 에이전트 researcher">')
    assert f"- 주장: {KEPT['claim']}" in result.content
    assert f"  출처: {KEPT['source']}" in result.content
    assert f"  인용: {LAW_QUOTE}" in result.content
    assert "찾지 못한 것:\n- 가짜법 제2조" in result.content
    assert result.content in [m.content for m in tool_messages(model.seen[3])]
    assert messages[-1].text == "가짜법 제1조는 목적 조항입니다 [가짜법 제1조 · 2000-01-01]"
    assert client.calls == [("POST", "/search/laws")]
    main_tools, sub_tools = set(model.bound[0]), set(model.bound[1])
    assert {"task", "judge", "write_todos", "read_file", "write_file"} <= main_tools
    assert not main_tools & RESEARCHER_TOOLS
    assert sub_tools == RESEARCHER_TOOLS


def test_result_line_without_source_is_dropped() -> None:
    script = [
        delegate("researcher", "가짜법 제1조를 찾아 줘", "t1"),
        answer([UNSOURCED, KEPT, {"claim": "출처 칸이 없는 가짜 주장"}, "글자만 있는 줄"]),
        AIMessage(content="끝"),
    ]
    messages, _, _ = asyncio.run(_ask(script))
    [result] = tool_messages(messages)
    assert KEPT["claim"] in result.content
    assert "출처 없는 가짜 주장" not in result.content
    assert "출처 칸이 없는 가짜 주장" not in result.content
    assert "글자만 있는 줄" not in result.content


def test_malformed_result_is_redelegated_once_then_marked_failed() -> None:
    script = [
        delegate("researcher", "가짜법 제1조를 찾아 줘", "t1"),
        AIMessage(content="형식 없이 쓴 가짜 답"),
        AIMessage(content="또 형식 없이 쓴 가짜 답"),
        AIMessage(content="끝"),
    ]
    messages, model, _ = asyncio.run(_ask(script))
    [result] = tool_messages(messages)
    assert result.content == f"{RESEARCH_FAIL}: researcher의 결과가 정해진 형식에 맞지 않음"
    first_task, second_task = model.seen[1][-1], model.seen[2][-1]
    assert isinstance(first_task, HumanMessage) and RETRY_NOTE not in first_task.content
    assert isinstance(second_task, HumanMessage) and second_task.content.endswith(RETRY_NOTE)
    assert model.calls == 4


class ConcurrencyModel(BaseChatModel):
    main_script: Any
    running: int = 0
    peak: int = 0
    finished: int = 0

    @property
    def _llm_type(self) -> str:
        return "concurrency-model"

    def bind_tools(self, tools: Any, **kwargs: Any) -> "ConcurrencyModel":
        return self

    def _generate(self, messages: Any, stop: Any = None, run_manager: Any = None, **kwargs: Any) -> ChatResult:
        raise NotImplementedError

    async def _agenerate(self, messages: Any, stop: Any = None, run_manager: Any = None, **kwargs: Any) -> ChatResult:
        system = messages[0].text if messages and isinstance(messages[0], SystemMessage) else ""
        if not system.startswith(RESEARCHER_PROMPT[:20]):
            return ChatResult(generations=[ChatGeneration(message=next(self.main_script))])
        self.running += 1
        self.peak = max(self.peak, self.running)
        await asyncio.sleep(0.05)
        self.running -= 1
        self.finished += 1
        return ChatResult(generations=[ChatGeneration(message=answer([KEPT]))])


@pytest.mark.parametrize(("limit", "expected_peak"), [(None, 2), (3, 3)])
def test_third_concurrent_subagent_waits(monkeypatch: pytest.MonkeyPatch, limit: int | None, expected_peak: int) -> None:
    if limit is not None:
        monkeypatch.setattr(build, "SubagentGate", lambda names: SubagentGate(names, limit=limit))
    three = AIMessage(
        content="",
        tool_calls=[
            {"name": "task", "args": {"description": f"가짜 조사 {i}", "subagent_type": "researcher"}, "id": f"t{i}"}
            for i in range(3)
        ],
    )
    model = ConcurrencyModel(main_script=iter([three, AIMessage(content="끝")]))

    async def scenario() -> list[Any]:
        agent, _, _ = await make_agent(model=model)
        result = await agent.ainvoke({"messages": [HumanMessage(content="세 갈래로 조사해 줘")]}, run_config())
        return result["messages"]

    messages = asyncio.run(scenario())
    results = tool_messages(messages)
    assert [m.tool_call_id for m in results] == ["t0", "t1", "t2"]
    assert all(KEPT["claim"] in m.content for m in results)
    assert model.finished == 3
    assert model.peak == expected_peak
