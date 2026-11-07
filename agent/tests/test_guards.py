import asyncio

from conftest import call, make_agent, run_config
from langchain_core.messages import AIMessage, HumanMessage, ToolMessage

from semantic_agent.agent.guards import REFUSAL, REPEAT_BLOCK, is_out_of_scope


def _tool_messages(messages: list) -> list[ToolMessage]:
    return [m for m in messages if isinstance(m, ToolMessage)]


def test_out_of_range_arg_is_blocked() -> None:
    async def scenario() -> None:
        agent, _, client = await make_agent([call("search_laws", {"query": "가짜", "k": 50}, "c1"), AIMessage(content="끝")])
        result = await agent.ainvoke({"messages": [HumanMessage(content="법령 찾아 줘")]}, run_config())
        [message] = _tool_messages(result["messages"])
        assert message.content.startswith("인자 오류")
        assert client.calls == []

    asyncio.run(scenario())


def test_third_identical_call_is_blocked() -> None:
    async def scenario() -> None:
        same = {"query": "가짜"}
        script = [call("search_laws", same, f"c{i}") for i in range(3)] + [AIMessage(content="끝")]
        agent, _, client = await make_agent(script)
        result = await agent.ainvoke({"messages": [HumanMessage(content="법령 찾아 줘")]}, run_config())
        contents = [m.content for m in _tool_messages(result["messages"])]
        assert contents[0].startswith('<자료 출처="') and contents[1].startswith('<자료 출처="')
        assert contents[2] == REPEAT_BLOCK
        assert len(client.calls) == 2

    asyncio.run(scenario())


def test_drafting_request_refused_and_definition_question_passes() -> None:
    assert is_out_of_scope("소장 써 줘")
    assert not is_out_of_scope("소장이 뭐야?")

    async def scenario() -> None:
        agent, model, _ = await make_agent([AIMessage(content="소장은 소를 제기할 때 내는 서면입니다 [가짜법 제1조]")])
        refused = await agent.ainvoke({"messages": [HumanMessage(content="소장 써 줘")]}, run_config())
        assert refused["messages"][-1].content == REFUSAL
        assert model.calls == 0
        passed = await agent.ainvoke({"messages": [HumanMessage(content="소장이 뭐야?")]}, run_config())
        assert passed["messages"][-1].content != REFUSAL
        assert model.calls == 1

    asyncio.run(scenario())
