from typing import Any

from langchain.tools import ToolRuntime
from langchain_core.tools import BaseTool, tool

from semantic_agent.tools.client import SEARCH_FAIL, Limit, ToolClient, material
from semantic_agent.tools.kg import session_id

TOOL_NAME = "search_document"
NOT_FOUND = "문서에서 맞는 부분을 찾지 못함"
DESCRIPTION = (
    "올린 문서에서 질문과 맞는 문단을 낱말 일치와 뜻 유사도로 찾아 쪽 번호와 함께 가져온다. "
    "큰 문서는 전체를 읽지 말고 이것으로 찾는다."
)


def _head(hit: dict[str, Any]) -> str:
    name = str(hit.get("filename") or "")
    page = hit.get("page")
    return f"{name} · {page}쪽" if isinstance(page, int) else name


def processing_line(names: list[str]) -> str:
    return f"아직 글자 인식 중: {', '.join(names)} — 끝나면 다시 찾을 수 있음"


def format_doc_hits(data: dict[str, Any]) -> str:
    lines = []
    processing = [str(name) for name in data.get("processing") or []]
    if processing:
        lines.append(processing_line(processing))
    hits = [hit for hit in data.get("hits") or [] if isinstance(hit, dict)]
    lines.extend(f"{_head(hit)}\n{hit.get('text', '')}" for hit in hits)
    if not hits:
        lines.append(NOT_FOUND)
    return "\n\n".join(lines)


def make_document_tool(client: ToolClient) -> BaseTool:
    @tool(TOOL_NAME, description=DESCRIPTION)
    async def search_document(query: str, runtime: ToolRuntime, k: Limit = 6) -> str:
        path = f"/sessions/{session_id(runtime)}/docs/search"
        body = {"query": query, "k": k}
        data, err = await client.fetch("POST", path, kind="search", fail=SEARCH_FAIL, json=body)
        if err:
            return err
        return material("문서 찾기", format_doc_hits(data if isinstance(data, dict) else {}))

    return search_document
