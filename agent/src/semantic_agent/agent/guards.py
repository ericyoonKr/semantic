import json
import logging
import re
from collections.abc import Awaitable, Callable
from typing import Any

from langchain.agents.middleware import AgentMiddleware, AgentState, hook_config
from langchain_core.messages import AIMessage, HumanMessage, ToolMessage
from langgraph.prebuilt.tool_node import ToolCallRequest
from langgraph.runtime import Runtime
from langgraph.types import Command
from pydantic import ValidationError

from semantic_agent.agent.injection import INJECTION_KIND, looks_injected
from semantic_agent.tools.client import material, unwrap_material
from semantic_agent.turn import current_turn

log = logging.getLogger(__name__)

REFUSAL = (
    "이 서비스는 법률 질문에 글로 설명하는 챗봇입니다. "
    "계약서·합의서·각서·위임장과 남이 발급하는 증서·증명서·확인서는 작성하거나 고쳐 드리지 않고, 파일도 만들어 드리지 않습니다. "
    "소장·답변서·준비서면 같은 소송 서면과 본인 명의의 간단한 공문·질의서는 간단한 초안을 글로 써 드릴 수 있고, 궁금한 법령·판례·요건은 출처와 함께 설명해 드리겠습니다."
)
REPEAT_BLOCK = "같은 호출 되풀이 — 다른 방법 필요"
REPEAT_LIMIT = 3
FILE_TOOL_PATH_ARG = {"ls": "path", "glob": "path", "grep": "path", "read_file": "file_path"}

DOCUMENT_WORDS = (
    "계약서", "고발장", "진정서", "탄원서", "내용증명",
    "합의서", "각서", "위임장", "행정심판청구서",
    "증서", "증명서", "확인서", "인증서",
)
TEMPLATE_WORDS = ("서식", "양식", "템플릿")
FILE_WORDS = ("파일", "다운로드", "내려받", "docx", "hwp", "hwpx", "pdf", "엑셀", "워드")
DRAFT = re.compile(
    r"(작성\s*해|작성|써|적어|만들어|짜|꾸며)\s*(줘|주세요|주십시오|줄래|줄\s*수|주라|달라|봐)"
    r"|작성\s*(부탁|요청)|대신\s*(작성|써)|초안"
)
GIVE = re.compile(r"(줘|주세요|주십시오|줄래|보여\s*줘|보내\s*줘|달라)")
FILE_VERBS = re.compile(r"(만들어|저장|내보내|변환|생성|뽑아|출력)")


def is_out_of_scope(text: str) -> bool:
    lowered = text.lower()
    if any(word in text for word in DOCUMENT_WORDS) and DRAFT.search(text):
        return True
    if any(word in text for word in TEMPLATE_WORDS) and GIVE.search(text):
        return True
    return any(word in lowered for word in FILE_WORDS) and bool(FILE_VERBS.search(text))


def _last_human_text(messages: list[Any]) -> str | None:
    for message in reversed(messages):
        if isinstance(message, HumanMessage):
            return message.content if isinstance(message.content, str) else str(message.content)
    return None


class ScopeGuard(AgentMiddleware):
    @hook_config(can_jump_to=["end"])
    async def abefore_agent(self, state: AgentState, runtime: Runtime) -> dict[str, Any] | None:
        text = _last_human_text(list(state.get("messages", [])))
        if text is None or not is_out_of_scope(text):
            return None
        log.info("guard_block kind=out_of_scope")
        current_turn().block("out_of_scope")
        return {"messages": [AIMessage(content=REFUSAL)], "jump_to": "end"}


def _call_key(name: str, args: Any) -> str:
    return name + "\x00" + json.dumps(args, sort_keys=True, ensure_ascii=False, default=str)


def repeat_count(messages: list[Any], tool_call_id: str, name: str, args: Any) -> int:
    start = 0
    for index in range(len(messages) - 1, -1, -1):
        if isinstance(messages[index], HumanMessage):
            start = index + 1
            break
    target = _call_key(name, args)
    count = 0
    for message in messages[start:]:
        if not isinstance(message, AIMessage):
            continue
        for call in message.tool_calls:
            if _call_key(call["name"], call["args"]) == target:
                count += 1
            if call.get("id") == tool_call_id:
                return count
    return count


def _arg_error(exc: ValidationError) -> str:
    parts = []
    for err in exc.errors():
        where = ".".join(str(p) for p in err.get("loc", ())) or "인자"
        parts.append(f"{where}: {err.get('msg', '')}")
    return "인자 오류: " + "; ".join(parts)


def wrap_file_result(name: str, args: Any, result: ToolMessage | Command) -> ToolMessage | Command:
    if name not in FILE_TOOL_PATH_ARG or not isinstance(result, ToolMessage) or result.status == "error":
        return result
    if not isinstance(result.content, str) or result.content.startswith("<자료 "):
        return result
    where = (args or {}).get(FILE_TOOL_PATH_ARG[name]) or "/"
    return result.model_copy(update={"content": material(f"파일 {where}", result.content)})


def result_text(result: ToolMessage | Command) -> str:
    if isinstance(result, ToolMessage):
        message: Any = result
    else:
        update = result.update if isinstance(result.update, dict) else {}
        messages = update.get("messages") or [None]
        message = messages[-1]
    content = getattr(message, "content", "")
    return content if isinstance(content, str) else str(content)


class ToolGuard(AgentMiddleware):
    async def awrap_tool_call(
        self,
        request: ToolCallRequest,
        handler: Callable[[ToolCallRequest], Awaitable[ToolMessage | Command]],
    ) -> ToolMessage | Command:
        call = request.tool_call
        name, args, call_id = call["name"], call.get("args", {}), call.get("id") or ""
        schema = getattr(request.tool, "tool_call_schema", None) if request.tool is not None else None
        if isinstance(schema, type) and hasattr(schema, "model_validate"):
            try:
                schema.model_validate(args)
            except ValidationError as exc:
                log.info("guard_block kind=tool_args tool=%s args=%s", name, str(args)[:160])
                current_turn().block("tool_args", tool=name)
                return ToolMessage(content=_arg_error(exc), tool_call_id=call_id, name=name, status="error")
        messages = list((request.state or {}).get("messages", []))
        if repeat_count(messages, call_id, name, args) >= REPEAT_LIMIT:
            log.info("guard_block kind=repeat tool=%s", name)
            current_turn().block("repeat", tool=name)
            return ToolMessage(content=REPEAT_BLOCK, tool_call_id=call_id, name=name, status="error")
        result = wrap_file_result(name, args, await handler(request))
        if looks_injected(unwrap_material(result_text(result))):
            log.info("injection_warning tool=%s", name)
            current_turn().warn_once(INJECTION_KIND)
        return result
