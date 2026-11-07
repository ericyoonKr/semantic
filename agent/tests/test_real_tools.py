import asyncio
import json
from typing import Any

import httpx
from conftest import call, make_agent, run_config, tool_messages
from langchain_core.messages import AIMessage, HumanMessage

from semantic_agent.api.sse import to_sse
from semantic_agent.config import Settings
from semantic_agent.tools.client import RustClient
from semantic_agent.tools.search import make_search_tools

KEY = "test-internal-key"
EDGES = {
    "edges": [
        {
            "from": "L3",
            "relation": "가짜위임",
            "certainty": "확실",
            "evidence": "가짜 근거",
            "neighbor": {"node": "L4", "name": "가짜 이웃", "content": "가짜 내용"},
        }
    ]
}


class FakeRust:
    def __init__(self) -> None:
        self.requests: list[httpx.Request] = []

    def handle(self, request: httpx.Request) -> httpx.Response:
        self.requests.append(request)
        path = request.url.path
        if path in ("/catalog", "/kg/catalog"):
            return httpx.Response(200, json=[])
        if path.endswith("/kg/expand"):
            return httpx.Response(200, json=EDGES)
        if path == "/search/cases":
            raise httpx.ReadTimeout("가짜 시간 초과", request=request)
        return httpx.Response(500, json={"message": "가짜 내부 오류"})

    def client(self) -> RustClient:
        settings = Settings(internal_key=KEY, rust_api_url="http://rust.test", fake_tools=False)
        return RustClient(settings, transport=httpx.MockTransport(self.handle))

    def find(self, suffix: str) -> httpx.Request:
        return next(r for r in self.requests if r.url.path.endswith(suffix))


def test_kg_expand_sends_session_path_short_id_and_headers() -> None:
    rust = FakeRust()
    script = [
        call("kg_expand", {"graph": "가짜그래프", "node": "L3", "direction": "out"}, "c1"),
        AIMessage(content="끝"),
    ]

    async def scenario() -> dict[str, Any]:
        agent, model, _ = await make_agent(script, client=rust.client())
        cfg = run_config(request_id="req-kg")
        [e async for e in to_sse(agent, "위임 관계", cfg, timeout_s=60)]
        return {"cfg": cfg, "seen": model.seen}

    out = asyncio.run(scenario())
    sid = out["cfg"]["configurable"]["thread_id"]
    assert [(r.method, r.url.path) for r in rust.requests[:2]] == [("GET", "/catalog"), ("GET", "/kg/catalog")]
    sent = rust.find("/kg/expand")
    assert (sent.method, sent.url.path) == ("POST", f"/sessions/{sid}/kg/expand")
    assert json.loads(sent.content) == {
        "graph": "가짜그래프",
        "node": "L3",
        "direction": "out",
        "relation": None,
        "depth": 1,
        "limit": 20,
    }
    assert sent.headers["x-internal-key"] == KEY
    assert sent.headers["x-request-id"] == "req-kg"
    [edge_result] = tool_messages(out["seen"][1])
    assert "L3 -[가짜위임 · 확실성 확실]-> L4 · 가짜 이웃" in edge_result.content


def test_http_500_and_timeout_become_failure_strings() -> None:
    rust = FakeRust()

    async def direct() -> tuple[str, str]:
        tools = {t.name: t for t in make_search_tools(rust.client())}
        laws = await tools["search_laws"].ainvoke({"query": "가짜"})
        cases = await tools["search_cases"].ainvoke({"query": "가짜"})
        return laws, cases

    laws, cases = asyncio.run(direct())
    assert laws.startswith("검색 실패: 지금 처리할 수 없음")
    assert cases.startswith("검색 실패: 시간 초과")

    script = [call("judge", {"claim_type": "가짜청구", "node_id": "R1_가짜요건"}, "c1"), AIMessage(content="끝")]

    async def judged() -> list[Any]:
        agent, _, _ = await make_agent(script, client=rust.client())
        result = await agent.ainvoke({"messages": [HumanMessage(content="성립하나?")]}, run_config())
        return result["messages"]

    [verdict] = tool_messages(asyncio.run(judged()))
    assert verdict.content.startswith("판정 불가: 지금 처리할 수 없음")
    assert rust.find("/judge").url.path.startswith("/sessions/")
