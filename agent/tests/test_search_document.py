import asyncio
from typing import Any

from conftest import ScriptedToolModel, call, make_agent, run_config, tool_messages
from langchain_core.messages import AIMessage, HumanMessage

from semantic_agent.tools.client import unwrap_material
from semantic_agent.tools.fake import FakeClient


class DocClient(FakeClient):
    def __init__(self, reply: dict[str, Any]) -> None:
        super().__init__()
        self.reply = reply
        self.bodies: list[Any] = []

    async def fetch(
        self,
        method: str,
        path: str,
        *,
        kind: str,
        fail: str,
        json: Any = None,
        params: dict[str, Any] | None = None,
        content: Any = None,
        headers: dict[str, str] | None = None,
    ) -> tuple[Any, str | None]:
        if not path.endswith("/docs/search"):
            return await super().fetch(
                method, path, kind=kind, fail=fail, json=json, params=params, content=content, headers=headers
            )
        self.calls.append((method, path))
        self.bodies.append(json)
        return self.reply, None


def _search(client: FakeClient, query: str = "계약 기간") -> tuple[str, str, ScriptedToolModel]:
    script = [call("search_document", {"query": query}, "d1"), AIMessage(content="끝")]

    async def scenario() -> tuple[list[Any], str, Any]:
        agent, model, _ = await make_agent(script, client=client)
        cfg = run_config()
        result = await agent.ainvoke({"messages": [HumanMessage(content="계약 기간은 얼마인가요?")]}, cfg)
        return result["messages"], cfg["configurable"]["thread_id"], model

    messages, thread_id, model = asyncio.run(scenario())
    [found] = [m for m in tool_messages(messages) if m.name == "search_document"]
    return str(found.content), thread_id, model


def test_hits_are_listed_under_file_name_and_page() -> None:
    client = FakeClient()
    content, thread_id, model = _search(client)
    assert "search_document" in model.bound[0]
    assert [c for c in client.calls if c[1].endswith("/docs/search")] == [("POST", f"/sessions/{thread_id}/docs/search")]
    assert content.startswith('<자료 출처="문서 찾기">')
    assert unwrap_material(content) == "fake.pdf · 1쪽\n가짜 문서의 고정 문단."


def test_processing_files_get_one_notice_line_and_null_page_has_no_page_mark() -> None:
    reply = {
        "hits": [{"file_id": "f1", "filename": "요건사실론.pdf", "page": None, "text": "쪽 표시 없는 문단."}],
        "processing": ["스캔본.pdf", "사진.pdf"],
        "by_meaning": False,
    }
    client = DocClient(reply)
    content, _, _ = _search(client, "요건사실")
    assert client.bodies == [{"query": "요건사실", "k": 6}]
    assert unwrap_material(content) == (
        "아직 글자 인식 중: 스캔본.pdf, 사진.pdf — 끝나면 다시 찾을 수 있음\n\n요건사실론.pdf\n쪽 표시 없는 문단."
    )


def test_empty_hits_say_nothing_matched() -> None:
    client = DocClient({"hits": [], "processing": [], "by_meaning": True})
    content, _, _ = _search(client)
    assert unwrap_material(content) == "문서에서 맞는 부분을 찾지 못함"
    assert "쪽" not in content
