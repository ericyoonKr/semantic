import asyncio
import json
from typing import Any

from conftest import answer, call, make_agent, run_config
from langchain_core.messages import AIMessage, ToolMessage

from semantic_agent.agent.research import ResearchOptions
from semantic_agent.api.sse import to_sse
from semantic_agent.tools.fake import FakeClient

QUOTE = "이 조문 전문은 가짜 도구가 돌려주는 고정 문장이다."


def _tokens(events: list[str]) -> str:
    return "".join(json.loads(e.split("data: ", 1)[1])["text"] for e in events if e.startswith("event: Token"))


async def _ask(script: list[AIMessage], question: str, client: FakeClient | None = None) -> tuple[list[str], Any]:
    use = client or FakeClient()
    agent, model, _ = await make_agent(script, client=use, research=ResearchOptions(first=True))
    events = [e async for e in to_sse(agent, question, run_config(), timeout_s=60, client=use)]
    return events, model


def test_the_step_from_the_search_server_runs_before_the_model() -> None:
    script = [
        call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
        AIMessage(content="가짜법 제1조는 목적 조항입니다 [근거 1].\n"),
    ]
    events, model = asyncio.run(_ask(script, "가짜법 제1조가 뭐야?"))
    assert any(e.startswith("event: Status") and '"tool": "task"' in e for e in events)
    assert any(isinstance(m, ToolMessage) and m.name == "task" for m in model.seen[-1])
    assert "가짜법 제1조(시행 2000-01-01)" in _tokens(events)
    assert model.calls == 3


def test_model_step_goes_straight_to_the_model() -> None:
    events, model = asyncio.run(_ask([AIMessage(content="네, 말씀하세요.\n")], "야"))
    assert not any('"tool": "task"' in e for e in events)
    assert model.calls == 1


def test_notes_from_the_search_server_open_the_answer_and_reach_the_model() -> None:
    from semantic_agent.tools.fake import NO_ORDINANCE_NOTE

    script = [
        call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
        AIMessage(content="가짜법 제1조는 목적 조항입니다 [근거 1].\n"),
    ]
    events, model = asyncio.run(_ask(script, "강남구 무슨 조례?"))
    text = _tokens(events)
    assert text.startswith(NO_ORDINANCE_NOTE), text
    assert "가짜법 제1조(시행 2000-01-01)" in text
    assert any("이번 차례 메모:" in str(getattr(m, "content", "")) for m in model.seen[-1])


def test_earlier_questions_and_the_last_answer_go_to_the_search_server() -> None:
    from langchain_core.messages import HumanMessage

    from semantic_agent.funcall.first_step import recent_turns
    from semantic_agent.tools.judge import UPLOAD_KIND

    messages = [
        HumanMessage(content="첫 질문"),
        AIMessage(content="첫 답"),
        HumanMessage(content="파일 올림", additional_kwargs={"kind": UPLOAD_KIND}),
        HumanMessage(content="둘째 질문"),
        AIMessage(content="", tool_calls=[{"name": "task", "args": {}, "id": "t1"}]),
        ToolMessage(content="x", tool_call_id="t1", name="task"),
        AIMessage(content="둘째 답"),
        HumanMessage(content="이번 질문"),
    ]
    assert recent_turns(messages) == (["첫 질문", "둘째 질문"], "둘째 답")

    async def two_turns() -> FakeClient:
        client = FakeClient()
        script = [
            AIMessage(content="네, 말씀하세요.\n"),
            call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
            answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
            AIMessage(content="가짜법 제1조는 목적 조항입니다 [근거 1].\n"),
        ]
        agent, _, _ = await make_agent(script, client=client, research=ResearchOptions(first=True))
        cfg = run_config()
        for question, request_id in (("야", "r1"), ("가짜법 제1조가 뭐야?", "r2")):
            cfg["configurable"]["request_id"] = request_id
            async for _ in to_sse(agent, question, cfg, timeout_s=60, client=client):
                pass
        return client

    client = asyncio.run(two_turns())
    first = [b for b in client.step_bodies if b["request_id"] == "r1"]
    second = [b for b in client.step_bodies if b["request_id"] == "r2"]
    assert first and first[0]["history"] == [] and "last_answer" not in first[0]
    assert second and second[0]["history"] == ["야"] and second[0]["last_answer"] == "네, 말씀하세요."


NO_SEARCH_NOTE = "앞서 검색도 읽기도 하지 않고 결과를 돌려줬다."
MUST_READ_NOTE = "올린 문서에 적힌 다음 조문·판례를 아직 읽지 않았다."
COVERAGE_NOTE = "이번 조사에서 아직 읽지 않은 근거 종류가 있다."


class ReviewingClient(FakeClient):
    def __init__(self, notes: list[str]) -> None:
        super().__init__()
        self.notes = list(notes)
        self.reviews: list[dict[str, Any]] = []

    def _next_step(self, body: dict[str, Any]) -> dict[str, Any]:
        step = super()._next_step(body)
        if step["tool"] == "task":
            step["args"]["description"] += "\n올린 문서에서 법령이 적힌 줄:\n「가짜법」 제2조를 어겼다."
        return step

    async def fetch(self, method: str, path: str, **kw: Any) -> tuple[Any, str | None]:
        if path == "/research/review":
            self.reviews.append(dict(kw["json"]))
            return {"note": self.notes.pop(0) if self.notes else None}, None
        return await super().fetch(method, path, **kw)


class JudgingClient(FakeClient):
    def __init__(self) -> None:
        super().__init__()
        self.judged: list[dict[str, Any]] = []

    async def fetch(self, method: str, path: str, **kw: Any) -> tuple[Any, str | None]:
        if path == "/research/judge":
            self.judged.append(dict(kw["json"]))
            refs = [i["ref"] for i in kw["json"]["items"]]
            return {"keep": refs[:1], "judged": True}, None
        return await super().fetch(method, path, **kw)


class QuoteRelaxingClient(FakeClient):
    def __init__(self) -> None:
        super().__init__()
        self.asked: list[dict[str, str]] = []

    async def fetch(self, method: str, path: str, **kw: Any) -> tuple[Any, str | None]:
        if path == "/research/quotes":
            self.asked.extend(kw["json"]["items"])
            return {"matched": [True for _ in kw["json"]["items"]]}, None
        return await super().fetch(method, path, **kw)


def test_quotes_the_search_server_accepts_are_kept_even_when_not_verbatim() -> None:
    client = QuoteRelaxingClient()
    loose = QUOTE.replace("고정 문장이다", "고정 문장…이다")
    script = [
        call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": loose}]),
        AIMessage(content="가짜법 제1조는 목적 조항입니다 [근거 1].\n"),
    ]
    _, model = asyncio.run(_ask(script, "가짜법 제1조가 뭐야?", client))
    assert client.asked and client.asked[0]["quote"] == loose
    task = next(m for m in model.seen[-1] if isinstance(m, ToolMessage) and m.name == "task")
    assert "목적 조항이다" in str(task.content) and "- 없음" not in str(task.content).split("찾지 못한 것")[0]


def test_sources_judged_off_the_issue_are_dropped_and_counted() -> None:
    client = JudgingClient()
    script = [
        call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
        call("read_case", {"decision_id": "1"}, "k1"),
        answer(
            [
                {"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE},
                {"claim": "엉뚱한 판례다", "source": "근거 2", "quote": "가짜 판결요지 고정 문장."},
            ]
        ),
        AIMessage(content="끝.\n"),
    ]
    events, model = asyncio.run(_ask(script, "가짜법 제1조가 뭐야?", client))
    assert len(client.judged) == 1 and [i["ref"] for i in client.judged[0]["items"]] == [1, 2]
    assert client.judged[0]["issue"].startswith("가짜법 제1조가 뭐야?")
    task = next(m for m in model.seen[-1] if isinstance(m, ToolMessage) and m.name == "task")
    assert "엉뚱한 판례다" not in str(task.content) and "목적 조항이다" in str(task.content)
    assert any('"irrelevant": 1' in e for e in events if e.startswith("event: Audit"))


def test_research_without_any_search_is_sent_back_inside_the_same_run() -> None:
    client = ReviewingClient([NO_SEARCH_NOTE])
    script = [
        answer([], ["찾지 못함"]),
        call("search_laws", {"queries": ["가짜법 목적"]}, "s1"),
        answer([], ["찾지 못함"]),
        AIMessage(content="조사한 자료로는 확인되지 않습니다.\n"),
    ]
    events, model = asyncio.run(_ask(script, "가짜법이 뭐야?", client))
    assert model.calls == 4
    assert [r["searched"] for r in client.reviews] == [False, True]
    assert client.reviews[0]["doc_lines"] == ["「가짜법」 제2조를 어겼다."]
    assert any(NO_SEARCH_NOTE in str(m.content) for m in model.seen[1])
    assert any(isinstance(m, ToolMessage) and m.name == "search_laws" for m in model.seen[2])
    assert _tokens(events).strip() == "조사한 자료로는 확인되지 않습니다."


def test_articles_named_in_the_document_must_be_read() -> None:
    client = ReviewingClient([f"{MUST_READ_NOTE} 가짜법 제2조"])
    script = [
        call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
        call("read_law", {"law": "가짜법", "unit": "제2조"}, "c2"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
        AIMessage(content="끝.\n"),
    ]
    _, model = asyncio.run(_ask(script, "무슨 법?", client))
    assert any(MUST_READ_NOTE in str(m.content) and "가짜법 제2조" in str(m.content) for m in model.seen[2])
    assert any(isinstance(m, ToolMessage) and m.name == "read_law" for m in model.seen[3])
    assert model.calls == 5


def test_research_without_a_case_is_sent_back_to_read_one() -> None:
    client = ReviewingClient([f"{COVERAGE_NOTE}\nsearch_cases로 쟁점의 판례를 찾아 읽는다."])
    script = [
        call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
        call("read_case", {"decision_id": "1"}, "k1"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
        AIMessage(content="끝.\n"),
    ]
    _, model = asyncio.run(_ask(script, "가짜법 제1조가 뭐야?", client))
    assert len(client.reviews) == 2
    assert any(COVERAGE_NOTE in str(m.content) and "search_cases" in str(m.content) for m in model.seen[2])
    assert model.calls == 5


def test_empty_final_answer_is_written_again() -> None:
    script = [
        call("search_laws", {"queries": ["가짜법"]}, "s1"),
        answer([], ["찾지 못함"]),
        AIMessage(content="<|channel>thought 정리 중<channel|>"),
        AIMessage(content="조사한 자료로는 확인되지 않습니다.\n"),
    ]
    events, model = asyncio.run(_ask(script, "아무 법이나 알려줘"))
    assert _tokens(events).strip() == "조사한 자료로는 확인되지 않습니다."
    assert model.calls == 4


def test_writing_guidance_from_the_search_server_reaches_the_model_but_not_the_answer() -> None:
    from semantic_agent.tools.fake import RULE_GUIDANCE

    script = [
        call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
        AIMessage(content="가짜법 제1조는 목적 조항입니다 [근거 1].\n"),
    ]
    events, model = asyncio.run(_ask(script, "학칙에 휴학 규정 있어?"))
    text = _tokens(events)
    assert text.startswith("가짜법 제1조"), text
    assert RULE_GUIDANCE not in text
    assert any(RULE_GUIDANCE in str(getattr(m, "content", "")) for m in model.seen[-1])


def test_the_first_researcher_call_is_the_graph_lookup_the_search_server_chose() -> None:
    class KgClient(FakeClient):
        def _next_step(self, body: dict[str, Any]) -> dict[str, Any]:
            step = super()._next_step(body)
            if step.get("tool") == "task":
                step["researcher_first"] = {"tool": "kg_find", "args": {"graph": "fake-graph", "text": "국가배상"}}
            return step

    script = [
        call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
        AIMessage(content="가짜법 제1조는 목적 조항입니다 [근거 1].\n"),
    ]
    events, model = asyncio.run(_ask(script, "국가배상 요건이 뭐야?", KgClient()))
    first_seen = [m.name for m in model.seen[0] if isinstance(m, ToolMessage)]
    assert first_seen == ["kg_find"], first_seen
    assert any(isinstance(m, ToolMessage) and m.name == "read_law" for m in model.seen[1])
    assert model.calls == 3
    assert "가짜법 제1조(시행 2000-01-01)" in _tokens(events)
