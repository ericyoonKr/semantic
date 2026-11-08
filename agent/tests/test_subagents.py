import asyncio
import json
from typing import Any

import pytest
from conftest import ScriptedToolModel, answer, call, delegate, make_agent, run_config, tool_messages
from langchain_core.language_models import BaseChatModel
from langchain_core.messages import AIMessage, HumanMessage, SystemMessage
from langchain_core.outputs import ChatGeneration, ChatResult
from pydantic import Field

from semantic_agent.agent import build
from semantic_agent.agent.delegation import RESEARCH_FAIL, SubagentGate
from semantic_agent.agent.prompts import ADVERSE_NOTE, RESEARCHER_PROMPT, RETRY_NOTE
from semantic_agent.agent.subagents import RESEARCHER_DESCRIPTION
from semantic_agent.ledger import LEDGER, Ledger
from semantic_agent.tools.fake import LAW_UNIT

LAW_BODY = LAW_UNIT["body"]
EXCERPT = "이 조문은 가짜 도구가 돌려주는 고정 문장이다."
KEPT = {"claim": "가짜법 제1조는 목적 조항이다", "source": "근거 1", "quote": LAW_BODY}
SHOWN_SOURCE = "[근거 1] 가짜법 제1조(시행 2000-01-01)"
UNSOURCED = {"claim": "출처 없는 가짜 주장", "source": "", "quote": LAW_BODY}
RESEARCHER_TOOLS = {"search_laws", "search_cases", "read_law", "read_case", "case_citations", "annex_lookup", "annex_compute"}
READ = {"law": "가짜법", "unit": "제1조"}


async def _ask(script: list[AIMessage]) -> tuple[list[Any], Any, Any]:
    LEDGER.set(Ledger())
    agent, model, client = await make_agent(script)
    result = await agent.ainvoke({"messages": [HumanMessage(content="가짜법 제1조가 뭐야?")]}, run_config())
    return result["messages"], model, client


def test_main_delegates_to_researcher_and_answers_from_result() -> None:
    script = [
        delegate("researcher", "쟁점: 가짜법 제1조의 내용\n목적: 조회", "t1"),
        call("read_law", READ, "c1"),
        answer([KEPT], ["가짜법 제2조"]),
        AIMessage(content="가짜법 제1조는 목적 조항입니다 [근거 1]"),
    ]
    messages, model, client = asyncio.run(_ask(script))
    [result] = tool_messages(messages)[-1:]
    assert result.name == "task"
    assert result.content.startswith('<자료 출처="하위 에이전트 researcher">')
    assert f"- 주장: {KEPT['claim']}" in result.content
    assert f"  출처: {SHOWN_SOURCE}" in result.content
    assert f"  인용: {LAW_BODY}" in result.content
    assert "찾지 못한 것:\n- 가짜법 제2조" in result.content
    assert client.calls == [("GET", "/laws/unit")]
    main_tools, sub_tools = set(model.bound[0]), set(model.bound[1])
    assert {"task", "judge", "write_todos", "read_file", "write_file"} <= main_tools
    assert not main_tools & RESEARCHER_TOOLS
    assert sub_tools == RESEARCHER_TOOLS


class TaskToolModel(ScriptedToolModel):
    task_text: list[str] = Field(default_factory=list)

    def bind_tools(self, tools: Any, **kwargs: Any) -> "TaskToolModel":
        self.task_text += [t.description for t in tools if getattr(t, "name", "") == "task"]
        return super().bind_tools(tools, **kwargs)


def test_main_holds_kg_tools_and_offers_only_researcher() -> None:
    script = [
        delegate("doc_reader", "계약 기간을 찾아 줘", "t1"),
        delegate("kg_explorer", "가짜 개념의 관계를 따라가 줘", "t2"),
        AIMessage(content="끝"),
    ]
    model = TaskToolModel(script=iter(script))

    async def scenario() -> tuple[list[Any], Any]:
        LEDGER.set(Ledger())
        agent, _, client = await make_agent(model=model)
        result = await agent.ainvoke({"messages": [HumanMessage(content="계약 기간과 개념 관계")]}, run_config())
        return result["messages"], client

    messages, client = asyncio.run(scenario())
    main_tools = set(model.bound[0])
    assert {"kg_find", "kg_expand", "task", "judge", "ls", "grep", "read_file"} <= main_tools
    assert not main_tools & RESEARCHER_TOOLS
    task_text = model.task_text[0]
    assert f"- researcher: {RESEARCHER_DESCRIPTION}" in task_text
    assert "doc_reader" not in task_text and "kg_explorer" not in task_text
    refused = [m.content for m in tool_messages(messages) if m.name == "task"]
    assert refused == [
        f"We cannot invoke subagent {name} because it does not exist, the only allowed types are `researcher`"
        for name in ("doc_reader", "kg_explorer")
    ]
    assert client.calls == []
    assert len(model.bound) == 3


def test_result_line_without_source_is_dropped() -> None:
    script = [
        delegate("researcher", "가짜법 제1조를 찾아 줘", "t1"),
        call("read_law", READ, "c1"),
        answer([UNSOURCED, KEPT, {"claim": "출처 칸이 없는 가짜 주장"}, "글자만 있는 줄"]),
        AIMessage(content="끝"),
    ]
    messages, _, _ = asyncio.run(_ask(script))
    result = tool_messages(messages)[-1]
    assert KEPT["claim"] in result.content
    assert "출처 없는 가짜 주장" not in result.content
    assert "출처 칸이 없는 가짜 주장" not in result.content
    assert "글자만 있는 줄" not in result.content


def test_fabricated_sources_and_quotes_are_dropped() -> None:
    lines = [
        {"claim": "없는 번호", "source": "근거 9", "quote": LAW_BODY},
        {"claim": "원문에 없는 인용", "source": "근거 1", "quote": "이 조문은 지어낸 문장이다."},
        {"claim": "법령명을 지어 쓴 출처", "source": "가짜법 제1조 · 2000-01-01", "quote": LAW_BODY},
        {"claim": "공백만 다른 인용", "source": "근거 1", "quote": "이 조문 전문은  가짜 도구가\n돌려주는 고정 문장이다."},
        {"claim": "공백을 끼운 인용", "source": "근거 1", "quote": "이 조문 전문은 가짜 도구 가 돌려주는 고정 문장 이다."},
    ]
    script = [
        delegate("researcher", "가짜법 제1조를 찾아 줘", "t1"),
        call("read_law", READ, "c1"),
        answer(lines),
        AIMessage(content="끝"),
    ]
    messages, _, _ = asyncio.run(_ask(script))
    result = tool_messages(messages)[-1].content
    assert "없는 번호" not in result
    assert "원문에 없는 인용" not in result
    assert "법령명을 지어 쓴 출처" not in result
    assert "공백만 다른 인용" in result
    assert "공백을 끼운 인용" in result


def test_search_excerpt_alone_cannot_be_cited() -> None:
    script = [
        delegate("researcher", "가짜법 제1조를 찾아 줘", "t1"),
        call("search_laws", {"queries": ["가짜법 제1조"]}, "c1"),
        answer([{"claim": "발췌만 본 주장", "source": "근거 1", "quote": EXCERPT}]),
        AIMessage(content="끝"),
    ]
    messages, _, _ = asyncio.run(_ask(script))
    result = tool_messages(messages)[-1].content
    assert "발췌만 본 주장" not in result


def _with_reverse(found: list[dict[str, Any]], reverse: str | None) -> AIMessage:
    msg = answer(found)
    data = json.loads(msg.content)
    data["reverse_query"] = reverse
    return AIMessage(content=json.dumps(data, ensure_ascii=False))


def test_invented_reverse_query_is_sent_back_until_real_search() -> None:
    support = {**KEPT, "stance": "support"}
    script = [
        delegate("researcher", "쟁점: 가짜 요건이 성립하는가\n목적: 판단", "t1"),
        call("read_law", READ, "c1"),
        _with_reverse([support], "지어낸 반대 검색어"),
        call("search_laws", {"queries": ["가짜 요건 불성립"]}, "c2"),
        _with_reverse([support], "가짜 요건 불성립"),
        AIMessage(content="끝"),
    ]
    messages, model, client = asyncio.run(_ask(script))
    result = tool_messages(messages)[-1].content
    assert "반대 방향 검색어: 가짜 요건 불성립" in result
    assert client.calls == [("GET", "/laws/unit"), ("POST", "/search/laws")]
    resent = [m for m in model.seen[3] if isinstance(m, HumanMessage)][-1]
    assert resent.content.endswith(ADVERSE_NOTE)


def test_reverse_query_backfilled_after_two_retries() -> None:
    support = {**KEPT, "stance": "support"}
    script = [
        delegate("researcher", "쟁점: 가짜 요건이 성립하는가\n목적: 판단", "t1"),
        call("read_law", READ, "c1"),
        _with_reverse([support], "지어낸 검색어 1"),
        _with_reverse([support], "지어낸 검색어 2"),
        _with_reverse([support], "지어낸 검색어 3"),
        AIMessage(content="끝"),
    ]
    messages, _, _ = asyncio.run(_ask(script))
    result = tool_messages(messages)[-1].content
    assert "지어낸 검색어" not in result
    assert "반대 방향 검색어" not in result


def test_lookup_purpose_skips_reverse_check() -> None:
    script = [
        delegate("researcher", "쟁점: 가짜법 제1조의 내용\n목적: 조회", "t1"),
        call("read_law", READ, "c1"),
        _with_reverse([KEPT], None),
        AIMessage(content="끝"),
    ]
    messages, model, _ = asyncio.run(_ask(script))
    assert KEPT["claim"] in tool_messages(messages)[-1].content
    assert model.calls == 4


def test_same_issue_is_not_delegated_twice() -> None:
    script = [
        delegate("researcher", "쟁점: 가짜법 제1조의 내용\n목적: 조회", "t1"),
        call("read_law", READ, "c1"),
        answer([KEPT]),
        delegate("researcher", "쟁점:  가짜법  제1조의 내용\n목적: 조회", "t2"),
        AIMessage(content="끝"),
    ]
    messages, model, client = asyncio.run(_ask(script))
    first, second = [m for m in tool_messages(messages) if m.name == "task"]
    assert first.content == second.content
    assert model.calls == 5
    assert client.calls == [("GET", "/laws/unit")]


def test_claim_type_issue_gets_tree_articles() -> None:
    script = [
        delegate("researcher", "쟁점: 가짜청구의 요건\n목적: 조회", "t1"),
        call("read_law", READ, "c1"),
        answer([KEPT]),
        AIMessage(content="끝"),
    ]
    _, model, _ = asyncio.run(_ask(script))
    task = [m for m in model.seen[1] if isinstance(m, HumanMessage)][-1]
    assert "먼저 읽을 조문: 가짜법 제1조" in task.content


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
        monkeypatch.setattr(build, "SubagentGate", lambda names, **kw: SubagentGate(names, limit=limit, **kw))
    three = AIMessage(
        content="",
        tool_calls=[
            {"name": "task", "args": {"description": f"가짜 조사 {i}", "subagent_type": "researcher"}, "id": f"t{i}"}
            for i in range(3)
        ],
    )
    model = ConcurrencyModel(main_script=iter([three, AIMessage(content="끝")]))

    async def scenario() -> list[Any]:
        ledger = Ledger()
        ledger.add_law(LAW_UNIT)
        LEDGER.set(ledger)
        agent, _, _ = await make_agent(model=model)
        result = await agent.ainvoke({"messages": [HumanMessage(content="세 갈래로 조사해 줘")]}, run_config())
        return result["messages"]

    messages = asyncio.run(scenario())
    results = tool_messages(messages)
    assert [m.tool_call_id for m in results] == ["t0", "t1", "t2"]
    assert all(KEPT["claim"] in m.content for m in results)
    assert model.finished == 3
    assert model.peak == expected_peak
