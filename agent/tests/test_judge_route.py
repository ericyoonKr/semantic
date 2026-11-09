import asyncio
from typing import Any

from langchain.agents.middleware import ModelRequest, ModelResponse
from langchain_core.language_models.fake_chat_models import GenericFakeChatModel
from langchain_core.messages import AIMessage, HumanMessage, SystemMessage, ToolMessage
from langchain_core.tools import tool

from semantic_agent.agent.judge_route import JudgeRoute, route_claim
from semantic_agent.turn import TURN, Turn

CLAIMS = {"CLAIM_국가배상": "국가배상"}


def test_router_picks_only_judgment_questions_about_a_catalog_claim() -> None:
    assert route_claim("올린 문서의 사고에 대해 구청에 국가배상책임이 성립하나?", CLAIMS) == "CLAIM_국가배상"
    assert route_claim("국가배상 소멸시효가 지났나?", CLAIMS) == "CLAIM_국가배상"
    assert route_claim("국가배상 요건이 뭐야?", CLAIMS) is None
    assert route_claim("국가배상법 제5조를 설명해 줘", CLAIMS) is None
    assert route_claim("취소소송 제소기간이 지났나?", CLAIMS) is None
    assert route_claim("국가배상이 성립하나?", {"CLAIM_국가배상": "국가배상", "CLAIM_국가": "국가"}) is None


@tool("judge", description="판정")
def judge(claim_type: str, node_id: str) -> str:
    return "판정"


def _request(messages: list[Any]) -> ModelRequest:
    model = GenericFakeChatModel(messages=iter([AIMessage(content="끝")]))
    return ModelRequest(model=model, messages=messages, system_message=SystemMessage(content="규칙"), tools=[judge])


def _route(messages: list[Any]) -> tuple[list[ModelRequest], ModelResponse]:
    got: list[ModelRequest] = []

    async def handler(request: ModelRequest) -> ModelResponse:
        got.append(request)
        return ModelResponse(result=[AIMessage(content="모델이 쓴 결론")])

    token = TURN.set(Turn())
    try:
        out = asyncio.run(JudgeRoute(CLAIMS.keys()).awrap_model_call(_request(messages), handler))
    finally:
        TURN.reset(token)
    return got, out


def test_judgment_question_gets_a_judge_call_from_code_until_judge_answers() -> None:
    asked = [HumanMessage(content="이 사고에 국가배상책임이 성립하나?")]
    got, out = _route(asked)
    assert got == []
    [msg] = out.result
    assert isinstance(msg, AIMessage)
    assert [(c["name"], c["args"]) for c in msg.tool_calls] == [("judge", {"claim_type": "CLAIM_국가배상", "node_id": "CLAIM_국가배상"})]
    answered = asked + [
        AIMessage(content="", tool_calls=[{"name": "judge", "args": {}, "id": "j1"}]),
        ToolMessage(content="판정", tool_call_id="j1", name="judge"),
    ]
    got, out = _route(answered)
    assert len(got) == 1 and out.result[0].content == "모델이 쓴 결론"
    got, _ = _route([HumanMessage(content="국가배상법 제2조 읽어 줘")])
    assert len(got) == 1


@tool("task", description="조사")
def task(description: str, subagent_type: str) -> str:
    return "조사"


def test_after_verdict_no_more_research_and_a_note_to_answer_from_the_verdict() -> None:
    asked = [
        HumanMessage(content="이 사고에 국가배상책임이 성립하나?"),
        AIMessage(content="", tool_calls=[{"name": "judge", "args": {}, "id": "j1"}]),
        ToolMessage(content="판정", tool_call_id="j1", name="judge"),
    ]
    model = GenericFakeChatModel(messages=iter([AIMessage(content="끝")]))
    request = ModelRequest(model=model, messages=asked, system_message=SystemMessage(content="규칙"), tools=[judge, task])
    got: list[ModelRequest] = []

    async def handler(r: ModelRequest) -> ModelResponse:
        got.append(r)
        return ModelResponse(result=[AIMessage(content="끝")])

    token = TURN.set(Turn())
    try:
        asyncio.run(JudgeRoute(CLAIMS.keys()).awrap_model_call(request, handler))
    finally:
        TURN.reset(token)
    assert [t.name for t in got[0].tools] == ["judge"]
    assert "더 조사하지 않는다" in str(got[0].system_message.content_blocks)
