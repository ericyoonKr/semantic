import asyncio
import json
from typing import Any

from conftest import answer, call, make_agent, run_config
from langchain_core.messages import AIMessage, ToolMessage

from semantic_agent.agent.prompts import MUST_READ_NOTE, NO_SEARCH_NOTE
from semantic_agent.api.app import record_upload
from semantic_agent.tools.fake import FakeClient
from semantic_agent.agent.research import ResearchOptions
from semantic_agent.agent.research_first import EXCERPT_HEAD, GREP, READ_FILE, doc_step, keyword, law_lines
from semantic_agent.api.sse import to_sse

QUOTE = "이 조문 전문은 가짜 도구가 돌려주는 고정 문장이다."


def _tokens(events: list[str]) -> str:
    return "".join(json.loads(e.split("data: ", 1)[1])["text"] for e in events if e.startswith("event: Token"))


async def _ask(script: list[AIMessage], question: str) -> tuple[list[str], object]:
    agent, model, client = await make_agent(script, research=ResearchOptions(first=True))
    events = [e async for e in to_sse(agent, question, run_config(), timeout_s=60, client=client)]
    return events, model


def test_research_runs_before_any_model_call() -> None:
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


def test_research_without_any_search_is_sent_back_once() -> None:
    script = [
        answer([], ["찾지 못함"]),
        call("search_laws", {"queries": ["가짜법 목적"]}, "s1"),
        answer([], ["찾지 못함"]),
        AIMessage(content="조사한 자료로는 확인되지 않습니다.\n"),
    ]
    events, model = asyncio.run(_ask(script, "가짜법이 뭐야?"))
    assert model.calls == 4
    assert any(NO_SEARCH_NOTE in str(m.content) for seen in model.seen for m in seen)
    assert _tokens(events).strip() == "조사한 자료로는 확인되지 않습니다."


def test_a_bare_call_word_goes_straight_to_the_model() -> None:
    events, model = asyncio.run(_ask([AIMessage(content="네, 말씀하세요.\n")], "야"))
    assert not any('"tool": "task"' in e for e in events)
    assert model.calls == 1
    assert keyword("야") is None and keyword("응?") is None


def test_small_documents_are_read_and_large_ones_are_searched() -> None:
    state = {
        "uploads": [
            {"path": "/uploads/big.md", "chars": 50_000},
            {"path": "/uploads/small.md", "chars": 3_000},
        ]
    }
    first = doc_step(state, [], 20_000, "낙찰률은 무엇인가?")
    assert first is not None and first.tool_calls[0]["name"] == READ_FILE
    assert first.tool_calls[0]["args"]["file_path"] == "/uploads/small.md"
    second = doc_step(state, [first], 20_000, "낙찰률은 무엇인가?")
    assert second is not None and second.tool_calls[0]["name"] == GREP
    assert second.tool_calls[0]["args"] == {"pattern": "낙찰률", "path": "/uploads/big.md", "output_mode": "content"}
    assert doc_step(state, [first, second], 20_000, "낙찰률은 무엇인가?") is None
    assert doc_step({"uploads": [{"path": "/uploads/big.md", "chars": 50_000}]}, [], 20_000, "법?") is None
    assert keyword("영업정지 처분은 무엇인가?") == "영업정지"


def test_research_brief_carries_the_lines_that_name_laws() -> None:
    state = {
        "uploads": [{"path": "/uploads/a.md", "chars": 100}],
        "files": {"/uploads/a.md": {"content": ["감사 결과", "「지방계약법」 제16조에 따라 검사한다.", "사진 첨부"]}},
    }
    assert law_lines(state) == "「지방계약법」 제16조에 따라 검사한다."
    from semantic_agent.agent.research_first import research_call

    brief = research_call("무슨 법?", law_lines(state)).tool_calls[0]["args"]["description"]
    assert brief.endswith(f"{EXCERPT_HEAD}\n「지방계약법」 제16조에 따라 검사한다.")


class UnreadOnce(FakeClient):
    def __init__(self) -> None:
        super().__init__()
        self.checked: list[list[str]] = []

    async def fetch(self, method: str, path: str, **kw: Any) -> tuple[Any, str | None]:
        if path == "/answer/check" and any("제2조" in s for s in kw["json"]["sentences"]):
            self.checked.append(kw["json"]["sentences"])
            unread = ["가짜법 제2조"] if len(self.checked) == 1 else []
            return {"results": [{"ok": not unread, "reasons": [], "unread": unread} for _ in kw["json"]["sentences"]]}, None
        return await super().fetch(method, path, **kw)


def test_articles_named_in_the_document_must_be_read() -> None:
    client = UnreadOnce()
    script = [
        call("read_law", {"law": "가짜법", "unit": "제1조"}, "c1"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
        call("read_law", {"law": "가짜법", "unit": "제2조"}, "c2"),
        answer([{"claim": "목적 조항이다", "source": "근거 1", "quote": QUOTE}]),
        AIMessage(content="끝.\n"),
    ]

    async def scenario() -> Any:
        agent, model, _ = await make_agent(script, client=client, research=ResearchOptions(first=True))
        cfg = run_config()
        await record_upload(agent, cfg, {"file_id": "aaa", "filename": "처분서.pdf", "pages": 1, "text": "감사 결과\n「가짜법」 제2조를 어겼다."})
        [e async for e in to_sse(agent, "무슨 법?", cfg, timeout_s=60, client=client)]
        return model

    model = asyncio.run(scenario())
    assert client.checked and client.checked[0] == ["「가짜법」 제2조를 어겼다."]
    assert any(MUST_READ_NOTE in str(m.content) and "가짜법 제2조" in str(m.content) for seen in model.seen for m in seen)
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
