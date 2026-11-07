from typing import Any

from langchain.tools import ToolRuntime
from langchain_core.messages import HumanMessage
from langchain_core.tools import BaseTool, tool
from pydantic import BaseModel

from semantic_agent.agent.authority import current_ledger
from semantic_agent.tools.client import JUDGE_FAIL, ToolClient, material, unwrap_material
from semantic_agent.tools.kg import session_id

UPLOAD_KIND = "upload"
VERDICT_HEAD = "판정:"
NEEDED_HEAD = "필요한 사실:"
PATH_HEAD = "경로:"


class Assumption(BaseModel):
    node_id: str
    value: bool
    said: str


def format_judge_catalog(data: Any) -> str:
    lines = []
    for item in data or []:
        lines.append(
            f"- {item.get('claim_type', '')} / {item.get('node_id', '')} — {item.get('label', '')}"
            f" ({item.get('gate', '')})"
        )
    return "\n".join(lines) or "쓸 수 있는 요건 트리 없음"


def is_upload_line(message: Any) -> bool:
    return isinstance(message, HumanMessage) and message.additional_kwargs.get("kind") == UPLOAD_KIND


def match_user_facts(messages: list[Any], facts: list[str]) -> tuple[list[dict[str, str]], list[str]]:
    said = [m for m in messages if isinstance(m, HumanMessage) and not is_upload_line(m)]
    found: list[dict[str, str]] = []
    missing: list[str] = []
    for fact in facts:
        source = next((m for m in said if isinstance(m.content, str) and fact in m.content), None)
        if source is None:
            missing.append(fact)
        else:
            found.append({"fact_id": f"F{len(found) + 1}", "text": fact, "message_id": str(source.id)})
    return found, missing


def format_verdict(data: dict[str, Any], missing: list[str]) -> str:
    parts = []
    if missing:
        parts.append("사용자 진술에서 찾지 못함: " + "; ".join(missing))
    parts.append(f"{VERDICT_HEAD}\n{data.get('text', '')}")
    needed = data.get("needed_facts") or []
    if needed:
        parts.append(NEEDED_HEAD + "\n" + "\n".join(f"- {n.get('ask', '')}" for n in needed))
    path = data.get("path") or []
    if path:
        parts.append(
            PATH_HEAD
            + "\n"
            + "\n".join(f"- {p.get('node_id', '')} {p.get('label', '')} ({p.get('gate', '')})" for p in path)
        )
    return "\n\n".join(parts)


def _section(text: str, head: str, stops: tuple[str, ...]) -> str:
    body = unwrap_material(text)
    start = body.find(head + "\n")
    if start < 0:
        return ""
    rest = body[start + len(head) + 1 :]
    cuts = [i for i in (rest.find("\n\n" + stop) for stop in stops) if i >= 0]
    return rest[: min(cuts)] if cuts else rest


def verdict_text(content: str) -> str:
    return _section(content, VERDICT_HEAD, (NEEDED_HEAD, PATH_HEAD))


def needed_asks(content: str) -> list[str]:
    block = _section(content, NEEDED_HEAD, (PATH_HEAD,))
    return [line[2:] for line in block.splitlines() if line.startswith("- ")]


def make_judge_tool(client: ToolClient, catalog_text: str) -> BaseTool:
    description = (
        "요건 판단을 한다. 성립하나·해당하나·지났나 같은 질문은 스스로 결론 내지 말고 이 도구를 부른다. "
        "user_facts에는 사용자가 말한 문장을 원문 그대로 넣는다.\n노드 카탈로그:\n" + catalog_text
    )

    @tool("judge", description=description)
    async def judge(
        claim_type: str,
        node_id: str,
        runtime: ToolRuntime,
        assumptions: list[Assumption] | None = None,
        user_facts: list[str] | None = None,
    ) -> str:
        found, missing = match_user_facts(list(runtime.state.get("messages", [])), list(user_facts or []))
        body = {
            "claim_type": claim_type,
            "node_id": node_id,
            "assumptions": [a.model_dump() if isinstance(a, BaseModel) else a for a in assumptions or []],
            "user_facts": found,
        }
        path = f"/sessions/{session_id(runtime)}/judge"
        data, err = await client.fetch("POST", path, kind="judge", fail=JUDGE_FAIL, json=body)
        if err:
            return err
        text = format_verdict(data, missing)
        current_ledger().trust(text)
        return material("요건 판단", text)

    return judge
