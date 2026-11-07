import asyncio

import pytest
from conftest import call, make_agent, run_config
from langchain_core.messages import AIMessage, ToolMessage

from semantic_agent.api.sse import to_sse
from semantic_agent.config import Settings

QUESTIONS = [
    ("가짜법 제1조의 내용이 뭐야?", "search_laws", {"query": "가짜법 제1조"}),
    ("이 쟁점에 관한 판례를 찾아 줘", "search_cases", {"query": "가짜 쟁점", "k": 5}),
    ("어제 다쳤다. 국가배상 요건이 성립하나?", "judge", {"claim_type": "가짜청구", "node_id": "R1_가짜요건", "user_facts": ["어제 다쳤다"]}),
]


@pytest.mark.parametrize(("question", "tool", "args"), QUESTIONS)
def test_question_picks_expected_fake_tool(question: str, tool: str, args: dict) -> None:
    async def scenario() -> None:
        agent, model, client = await make_agent([call(tool, args, "c1"), AIMessage(content="가짜 답변 [가짜법 제1조]")])
        cfg = run_config(Settings(internal_key="k", fake_tools=True).recursion_limit)
        events = [e async for e in to_sse(agent, question, cfg, timeout_s=60)]
        assert any(e.startswith("event: Status") and f'"tool": "{tool}"' in e for e in events)
        assert events[-1].startswith("event: Done")
        messages = (await agent.aget_state(cfg)).values["messages"]
        tool_messages = [m for m in messages if isinstance(m, ToolMessage)]
        assert [m.name for m in tool_messages] == [tool]
        assert tool_messages[0].content.startswith('<자료 출처="')
        assert messages[-1].text == "가짜 답변 [가짜법 제1조]"
        assert model.calls == 2

    asyncio.run(scenario())
