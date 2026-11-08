import asyncio
import json
from typing import Any

import httpx
from conftest import call, make_agent, run_config, tool_messages
from langchain_core.messages import AIMessage, HumanMessage

from semantic_agent.config import Settings
from semantic_agent.tools.client import RustClient
from semantic_agent.tools.versions import format_citations, format_resolve, format_trace

VERSION = {
    "unit": {"law_id": "001638", "kind": "Article", "key": "제156조"},
    "promul_no": {"issuer": "법률", "number": 18522},
    "promul_date": "2021-11-30",
    "valid_period": {"from": "2022-12-01", "until": "2023-07-04"},
    "end_reason": "Amended",
    "body": "가짜 본문",
}
RESOLVE = {
    "law": {"law_id": "001638", "name": "도로교통법", "law_type": "법률"},
    "unit": {"law_id": "001638", "kind": "Article", "key": "제156조"},
    "as_of": "2023-06-01",
    "data_from": "2022-04-20",
    "resolution": {
        "status": "Undetermined",
        "versions": [VERSION],
        "trace": [
            {"RefTime": {"principle": {"id": "P:행위시법", "label": "행위시법", "quotes": []}, "time_id": "T:행위시",
                         "date": {"Point": "2023-01-01"}}}
        ],
        "flags": [
            {"AddendumUndetermined": {"rule_id": "R-1", "note": "부칙 규칙이 구조화되지 않아 평가하지 못함"}},
            {"DataHold": {"unit": {"key": "제156조"}, "note": "자료 시작 앞"}},
        ],
        "missing": [{"event": "Judgment", "reason": "Absent", "step": "Adjust"}],
        "needs_expert": False,
    },
}
CITATIONS = {
    "judgment_date": "2023-03-01",
    "items": [
        {"law_name": "도로교통법", "old": True, "cite_type": "UpperOnly", "article": "제156조",
         "resolution": {"status": "Resolved", "version": VERSION, "base_date": "2022-07-11", "flags": ["AssumedCurrent"]}},
        {"law_name": "도로교통법", "old": True, "cite_type": "UpperWithEffective", "article": "제148조의2 제1항",
         "resolution": {"status": {"OutsideData": {"first": "2022-04-20", "why": {"NoVersionAt": "2020-12-09"}}},
                        "flags": []}},
        {"law_name": "조세감면규제법", "old": True, "cite_type": "AsPromulgated", "article": "부칙 제23조",
         "unresolved": "조 단위가 아닌 인용"},
    ],
    "truncated": False,
}


def test_resolution_text_names_status_flags_missing_and_chain() -> None:
    text = format_resolve(RESOLVE)
    assert text.splitlines()[0] == "[도로교통법 제156조 · 법령ID 001638 · 판단 기준일 2023-06-01 · 판본 자료 시작 2022-04-20]"
    assert "상태: 판정 보류" in text
    assert "- 제156조 · 법률 제18522호(2021-11-30 공포) · 시행 2022-12-01~2023-07-04 · 종료 Amended" in text
    assert "- 부칙 미평가(판정 보류): 부칙 규칙이 구조화되지 않아 평가하지 못함 [R-1]" in text
    assert "- 자료 부족(판정 보류): 자료 시작 앞" in text
    assert "- Judgment: 없음" in text
    assert "- 기준시: 행위시법 → T:행위시" in text
    assert "전문가 확인 필요" not in text


def test_not_judged_status_is_named() -> None:
    text = format_resolve({"resolution": {"status": {"NotJudged": "NoPrinciple"}}})
    assert "상태: 판단 안 함 — 이 분야에 기준시 원칙이 없음" in text


def test_citation_lines_say_resolved_outside_and_unresolved() -> None:
    lines = format_citations(CITATIONS).splitlines()
    assert lines[1].startswith("- 구 도로교통법 제156조 · UpperOnly → 해소 · 기준일 2022-07-11 · 제156조 · 법률 제18522호")
    assert "무표시 인용이라 선고일 현행으로 봄" in lines[1]
    assert "판본 자료 범위 밖(자료 시작 2022-04-20) — 오기로 보지 않음" in lines[2]
    assert lines[3] == "- 구 조세감면규제법 부칙 제23조 · AsPromulgated → 해소 안 함: 조 단위가 아닌 인용"


def test_trace_text_shows_both_lanes_and_outcome() -> None:
    data = {
        "judgment_date": "2023-03-01",
        "counts": {"same": 1, "no_answer": 1},
        "rows": [
            {"answer": CITATIONS["items"][0], "prediction": RESOLVE["resolution"], "outcome": "Same"},
            {"answer": CITATIONS["items"][2], "prediction": None, "outcome": "NoAnswer"},
        ],
    }
    text = format_trace(data)
    assert "대조: no_answer 1, same 1" in text
    assert "   예측 줄: 판정 보류 · 제156조 · 법률 제18522호" in text
    assert "   결과: 일치" in text
    assert "   결과: 답변 줄 없음" in text


class FakeRust:
    def __init__(self) -> None:
        self.requests: list[httpx.Request] = []

    def handle(self, request: httpx.Request) -> httpx.Response:
        self.requests.append(request)
        path = request.url.path
        if path in ("/catalog", "/kg/catalog"):
            return httpx.Response(200, json=[])
        if path == "/versions/resolve":
            return httpx.Response(200, json=RESOLVE)
        return httpx.Response(500, json={"error": "가짜 내부 오류"})

    def client(self) -> RustClient:
        settings = Settings(internal_key="test-internal-key", rust_api_url="http://rust.test", fake_tools=False)
        return RustClient(settings, transport=httpx.MockTransport(self.handle))


def test_resolve_version_tool_sends_facts_with_from_and_to() -> None:
    rust = FakeRust()
    args = {
        "law": "도로교통법",
        "unit": "제156조",
        "kind": "CriminalSubstantive",
        "facts": [{"event": "Act", "start": "2020-01-01", "end": "2020-01-31"}],
        "as_of": "2023-06-01",
    }
    script = [call("resolve_version", args, "c1"), AIMessage(content="끝")]

    async def scenario() -> dict[str, Any]:
        agent, _, _ = await make_agent(script, client=rust.client())
        return await agent.ainvoke({"messages": [HumanMessage(content="판본")]}, run_config(request_id="req-v"))

    result = asyncio.run(scenario())
    sent = next(r for r in rust.requests if r.url.path == "/versions/resolve")
    body = json.loads(sent.content)
    assert body["facts"] == [{"event": "Act", "on": None, "from": "2020-01-01", "to": "2020-01-31", "continuing": False}]
    assert body["kind"] == "CriminalSubstantive" and body["as_of"] == "2023-06-01"
    reply = tool_messages(result["messages"])[0].content
    assert "상태: 판정 보류" in reply
