import operator
from collections.abc import Awaitable, Callable
from typing import Annotated, Any, NotRequired

from langchain.agents.middleware import AgentMiddleware, AgentState, ModelRequest, ModelResponse

from semantic_agent.agent.context import append_system

PROGRESS_KEY = "kg_progress"
PROGRESS_HEAD = "## 사건 진행 상태 (이 대화 안에서만)"
MAX_LINES = 30
STATUS_LABELS = {"proposed": "제안", "confirmed": "확인됨"}
CHANGED = "그래프가 바뀜 — 다시 확인"


class ProgressState(AgentState):
    kg_progress: NotRequired[Annotated[list[dict[str, Any]], operator.add]]


def fold(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    kept: dict[tuple[str, str], list[dict[str, Any]]] = {}
    for row in rows:
        key = (str(row.get("graph", "")), str(row.get("node", "")))
        bucket = kept.setdefault(key, [])
        if row.get("status") == "retracted":
            kept[key] = [r for r in bucket if r.get("value") != row.get("value")]
        else:
            bucket.append(row)
    out = []
    for bucket in kept.values():
        confirmed = [r for r in bucket if r.get("status") == "confirmed"]
        if bucket:
            out.append((confirmed or bucket)[-1])
    return out


def progress_text(rows: list[dict[str, Any]], hashes: dict[str, str]) -> str:
    lines = []
    for row in fold(rows)[-MAX_LINES:]:
        line = (
            f"- {row.get('node', '')} {row.get('name', '')} ({row.get('graph', '')}): {row.get('value', '')}"
            f" · {STATUS_LABELS.get(str(row.get('status')), row.get('status'))}"
            f" · 근거 「{row.get('basis', '')}」 · {row.get('turn', '')}번째 질문"
        )
        current = hashes.get(str(row.get("graph", "")))
        if current is None or current != row.get("sha256"):
            line += f" · {CHANGED}"
        lines.append(line)
    return "\n".join(lines)


class ProgressMiddleware(AgentMiddleware):
    state_schema = ProgressState

    def __init__(self, hashes: dict[str, str]) -> None:
        super().__init__()
        self._hashes = dict(hashes)

    async def awrap_model_call(
        self,
        request: ModelRequest,
        handler: Callable[[ModelRequest], Awaitable[ModelResponse]],
    ) -> ModelResponse:
        text = progress_text(list(request.state.get(PROGRESS_KEY) or []), self._hashes)
        if not text:
            return await handler(request)
        block = f"{PROGRESS_HEAD}\n{text}"
        return await handler(request.override(system_message=append_system(request.system_message, block)))
