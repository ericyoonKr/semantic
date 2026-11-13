import asyncio
import contextlib
import logging
import re
import secrets
import uuid
from contextlib import asynccontextmanager
from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import Any

import uvicorn
from deepagents.backends.utils import create_file_data, file_data_to_string
from fastapi import Depends, FastAPI, Header, HTTPException, Request, Response
from fastapi.responses import JSONResponse, StreamingResponse
from starlette.background import BackgroundTask
from langchain_core.language_models import BaseChatModel
from langchain_core.messages import AIMessage, HumanMessage, ToolMessage
from langgraph.checkpoint.base import BaseCheckpointSaver
from langgraph.store.base import BaseStore
from pydantic import BaseModel, Field

from semantic_agent.agent.build import build_agent, load_catalogs
from semantic_agent.agent.research import ResearchOptions
from semantic_agent.agent.model import build_model
from semantic_agent.api.sessions import SessionRegistry, run_ask, run_upload
from semantic_agent.api.answers import (
    delete_answers,
    delete_ledger,
    last_answer_id,
    load_ledger,
    save_answer,
    save_ledger,
    saved_answers,
)
from semantic_agent.api.runlog import RunLog
from semantic_agent.ledger import Ledger
from semantic_agent.api.sse import event, to_sse
from semantic_agent.config import Settings
from semantic_agent.logs import setup_logging
from semantic_agent.memory.store import delete_prefs, delete_session, open_checkpointer, open_pool, open_store
from semantic_agent.tools.client import RustClient, ToolClient
from semantic_agent.tools.fake import FakeClient
from semantic_agent.tools.judge import UPLOAD_KIND, is_upload_line, verdict_text

BIND_HOST = "127.0.0.1"
BIND_PORT = 8000
UPLOAD_FAIL = "업로드 실패"

log = logging.getLogger(__name__)


class AskBody(BaseModel):
    question: str = Field(min_length=1)


@dataclass
class AppState:
    agent: Any
    saver: BaseCheckpointSaver
    store: BaseStore
    client: ToolClient
    registry: SessionRegistry
    background: set[asyncio.Task[Any]] = field(default_factory=set)


def _uuid(value: str | None) -> str | None:
    if not value:
        return None
    try:
        return str(uuid.UUID(value))
    except ValueError:
        return None


def _session(session_id: str) -> str:
    parsed = _uuid(session_id)
    if parsed is None:
        raise HTTPException(status_code=404, detail="없는 대화")
    return parsed


def user_id_header(x_user_id: str | None = Header(default=None)) -> str:
    parsed = _uuid(x_user_id)
    if parsed is None:
        raise HTTPException(status_code=400, detail="사용자 ID 머리가 없습니다")
    return parsed


def _config(settings: Settings, session_id: str, user_id: str) -> dict[str, Any]:
    return {
        "configurable": {"thread_id": session_id, "user_id": user_id, "request_id": uuid.uuid4().hex},
        "recursion_limit": settings.recursion_limit,
    }


def history_items(messages: list[Any], saved: dict[str, dict[str, Any]] | None = None) -> list[dict[str, Any]]:
    saved = saved or {}
    items: list[dict[str, Any]] = []
    for message in messages:
        if is_upload_line(message):
            items.append({"role": "upload", "text": str(message.content)})
        elif isinstance(message, HumanMessage):
            items.append({"role": "user", "text": str(message.content)})
        elif isinstance(message, ToolMessage) and message.name == "judge":
            text = verdict_text(str(message.content))
            if text:
                items.append({"role": "verdict", "text": text})
        elif isinstance(message, AIMessage) and not message.tool_calls and message.text:
            kept = saved.get(message.id or "")
            if kept:
                items.append({"role": "assistant", "text": kept["text"], "sources": kept.get("sources", [])})
            else:
                items.append({"role": "assistant", "text": message.text})
    return items


def upload_line(data: dict[str, Any]) -> str:
    line = f"[업로드] {data.get('filename', '')}, {data.get('pages', 0)}쪽"
    warnings = [str(w) for w in data.get("warnings") or []]
    return line + (f", 추출 불가: {'; '.join(warnings)}" if warnings else "")


FILE_ID = re.compile(r"^[A-Za-z0-9_-]{1,80}$")


def uploaded_files(values: dict[str, Any]) -> list[dict[str, Any]]:
    keep = ("file_id", "filename", "format", "pages", "uploaded_at")
    return [{k: u.get(k) for k in keep} for u in values.get("uploads") or [] if isinstance(u, dict)]


PASSED_HEADERS = ("content-type", "content-length", "x-file-name")


def known_upload(values: dict[str, Any], file_id: str) -> bool:
    return bool(FILE_ID.match(file_id)) and any(u.get("file_id") == file_id for u in uploaded_files(values))


def uploaded_text(values: dict[str, Any], file_id: str) -> dict[str, Any] | None:
    if not FILE_ID.match(file_id):
        return None
    meta = next((u for u in uploaded_files(values) if u.get("file_id") == file_id), None)
    data = (values.get("files") or {}).get(f"/uploads/{file_id}.md")
    if meta is None or not data:
        return None
    return {**meta, "text": file_data_to_string(data)}


async def record_upload(agent: Any, cfg: dict[str, Any], data: dict[str, Any]) -> None:
    path = f"/uploads/{data['file_id']}.md"
    state = await agent.aget_state(cfg)
    uploads = list(state.values.get("uploads") or [])
    uploads.append(
        {
            "file_id": data["file_id"],
            "filename": data.get("filename", ""),
            "format": data.get("format", ""),
            "pages": data.get("pages", 0),
            "uploaded_at": datetime.now(UTC).isoformat(timespec="seconds"),
            "warnings": list(data.get("warnings") or []),
            "revision_of": data.get("revision_of"),
            "outline": list(data.get("outline") or []),
            "path": path,
            "chars": len(str(data.get("text", ""))),
        }
    )
    marker = HumanMessage(content=upload_line(data), additional_kwargs={"kind": UPLOAD_KIND})
    await agent.aupdate_state(
        cfg,
        {"files": {path: create_file_data(str(data.get("text", "")))}, "uploads": uploads, "messages": [marker]},
        as_node="model",
    )


async def keep_answer(state: "AppState", cfg: dict[str, Any], user_id: str, sid: str, items: list[str]) -> None:
    try:
        snapshot = await state.agent.aget_state(cfg)
        message_id = last_answer_id(list(snapshot.values.get("messages", [])))
        if message_id:
            await save_answer(state.store, user_id, sid, message_id, items)
    except Exception as exc:
        log.warning("answer not kept type=%s", type(exc).__name__)


async def keep_ledger(state: "AppState", user_id: str, sid: str, ledger: Ledger) -> None:
    try:
        await save_ledger(state.store, user_id, sid, ledger)
    except Exception as exc:
        log.warning("ledger not kept type=%s", type(exc).__name__)


def create_app(
    settings: Settings | None = None,
    *,
    model: BaseChatModel | None = None,
    saver: BaseCheckpointSaver | None = None,
    store: BaseStore | None = None,
    client: ToolClient | None = None,
) -> FastAPI:
    settings = settings or Settings()

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        missing = settings.missing()
        if missing:
            raise RuntimeError("설정 값이 없습니다: " + ", ".join(missing))
        pool = None
        use_saver, use_store = saver, store
        if use_saver is None or use_store is None:
            pool = await open_pool(settings)
            use_saver = await open_checkpointer(pool)
            use_store = await open_store(pool)
        use_client = client or (FakeClient() if settings.fake_tools else RustClient(settings))
        catalogs = await load_catalogs(use_client)
        agent = build_agent(
            model or build_model(settings),
            use_client,
            catalogs,
            use_saver,
            use_store,
            ResearchOptions.from_settings(settings),
        )
        registry = SessionRegistry()
        app.state.rt = AppState(agent, use_saver, use_store, use_client, registry)
        try:
            yield
        finally:
            registry.cancel_all()
            await use_client.aclose()
            if pool is not None:
                await pool.close()

    def require_internal_key(x_internal_key: str | None = Header(default=None)) -> None:
        expected = settings.internal_key.get_secret_value().encode()
        given = (x_internal_key or "").encode()
        if not expected or not secrets.compare_digest(given, expected):
            raise HTTPException(status_code=403, detail="거부")

    app = FastAPI(lifespan=lifespan, dependencies=[Depends(require_internal_key)], docs_url=None, redoc_url=None, openapi_url=None)

    def rt(request: Request) -> AppState:
        return request.app.state.rt

    @app.post("/session")
    async def create_session(user_id: str = Depends(user_id_header)) -> dict[str, str]:
        return {"session_id": str(uuid.uuid4())}

    @app.post("/session/{session_id}/upload")
    async def upload(session_id: str, request: Request, user_id: str = Depends(user_id_header)) -> JSONResponse:
        sid = _session(session_id)
        state = rt(request)
        cfg = _config(settings, sid, user_id)

        async def work() -> tuple[Any, str | None]:
            headers = {"content-type": request.headers.get("content-type", "application/octet-stream")}
            data, err = await state.client.fetch(
                "POST", f"/sessions/{sid}/files", kind="judge", fail=UPLOAD_FAIL, content=request.stream(), headers=headers
            )
            if err is None:
                await record_upload(state.agent, cfg, data)
            return data, err

        data, err = await run_upload(state.registry.get(sid), work())
        if err is not None:
            return JSONResponse(status_code=502, content={"message": err})
        summary = {
            "file_id": data["file_id"],
            "filename": data.get("filename", ""),
            "format": data.get("format", ""),
            "pages": data.get("pages", 0),
            "outline": data.get("outline", []),
            "warnings": data.get("warnings", []),
            "findings_count": len(data.get("findings") or []),
        }
        return JSONResponse(content=summary)

    @app.post("/session/{session_id}/ask")
    async def ask(session_id: str, body: AskBody, request: Request, user_id: str = Depends(user_id_header)) -> StreamingResponse:
        sid = _session(session_id)
        state = rt(request)
        cfg = _config(settings, sid, user_id)
        request_id = cfg["configurable"]["request_id"]
        slot = state.registry.get(sid)
        log = RunLog(body.question)

        async def produce() -> None:
            slot.log = log
            if not slot.ledger_loaded:
                with contextlib.suppress(Exception):
                    slot.ledger = await load_ledger(state.store, user_id, sid)
                slot.ledger_loaded = True
            try:
                async for item in to_sse(state.agent, body.question, cfg, settings.ask_timeout_s, state.client, slot.ledger):
                    log.add(item)
                await keep_answer(state, cfg, user_id, sid, log.items)
                await keep_ledger(state, user_id, sid, slot.ledger)
            except asyncio.CancelledError:
                log.add(event("Error", {"message": "질문이 중단되었습니다", "request_id": request_id}))
                raise
            finally:
                log.finish()

        state.background.add(task := asyncio.create_task(run_ask(slot, produce())))
        task.add_done_callback(state.background.discard)
        return StreamingResponse(log.follow(), media_type="text/event-stream")

    @app.get("/session/{session_id}/live")
    async def live(session_id: str, request: Request, user_id: str = Depends(user_id_header)) -> Response:
        sid = _session(session_id)
        log = rt(request).registry.live(sid)
        if log is None:
            return Response(status_code=204)

        async def replay():
            yield event("Resume", {"question": log.question})
            async for item in log.follow():
                yield item

        return StreamingResponse(replay(), media_type="text/event-stream")

    @app.get("/session/{session_id}/history")
    async def history(session_id: str, request: Request, user_id: str = Depends(user_id_header)) -> dict[str, Any]:
        sid = _session(session_id)
        state = rt(request)
        snapshot = await state.agent.aget_state(_config(settings, sid, user_id))
        saved = await saved_answers(state.store, user_id, sid)
        return {"messages": history_items(list(snapshot.values.get("messages", [])), saved)}

    @app.get("/session/{session_id}/files")
    async def files(session_id: str, request: Request, user_id: str = Depends(user_id_header)) -> dict[str, Any]:
        sid = _session(session_id)
        snapshot = await rt(request).agent.aget_state(_config(settings, sid, user_id))
        return {"files": uploaded_files(snapshot.values)}

    @app.get("/session/{session_id}/files/{file_id}")
    async def file_text(session_id: str, file_id: str, request: Request, user_id: str = Depends(user_id_header)) -> JSONResponse:
        sid = _session(session_id)
        snapshot = await rt(request).agent.aget_state(_config(settings, sid, user_id))
        found = uploaded_text(snapshot.values, file_id)
        if found is None:
            return JSONResponse(status_code=404, content={"message": "파일이 없습니다"})
        return JSONResponse(content=found)

    @app.get("/session/{session_id}/files/{file_id}/original")
    async def file_original(session_id: str, file_id: str, request: Request, user_id: str = Depends(user_id_header)) -> Response:
        sid = _session(session_id)
        state = rt(request)
        snapshot = await state.agent.aget_state(_config(settings, sid, user_id))
        if not known_upload(snapshot.values, file_id):
            return JSONResponse(status_code=404, content={"message": "파일이 없습니다"})
        upstream = await state.client.open_bytes(f"/sessions/{sid}/files/{file_id}/original", kind="search")
        if upstream is None:
            return JSONResponse(status_code=502, content={"message": "원본을 가져오지 못했습니다"})
        if upstream.status_code != 200:
            await upstream.aclose()
            status = 404 if upstream.status_code == 404 else 502
            return JSONResponse(status_code=status, content={"message": "원본을 가져오지 못했습니다"})
        headers = {k: upstream.headers[k] for k in PASSED_HEADERS if k in upstream.headers}
        return StreamingResponse(upstream.aiter_bytes(), headers=headers, background=BackgroundTask(upstream.aclose))

    @app.delete("/session/{session_id}", status_code=204)
    async def remove_session(session_id: str, request: Request, user_id: str = Depends(user_id_header)) -> Response:
        sid = _session(session_id)
        state = rt(request)
        await delete_session(state.saver, sid)
        await delete_answers(state.store, user_id, sid)
        await delete_ledger(state.store, user_id, sid)
        _, err = await state.client.fetch("DELETE", f"/sessions/{sid}", kind="search", fail="세션 삭제 실패")
        state.registry.drop(sid)
        if err is not None:
            return JSONResponse(status_code=502, content={"message": err})
        return Response(status_code=204)

    @app.delete("/prefs", status_code=204)
    async def remove_prefs(request: Request, user_id: str = Depends(user_id_header)) -> Response:
        await delete_prefs(rt(request).store, user_id)
        return Response(status_code=204)

    return app


def main() -> None:
    settings = Settings()
    setup_logging(settings.data_dir)
    uvicorn.run(create_app(settings), host=BIND_HOST, port=BIND_PORT, workers=1, log_config=None)


if __name__ == "__main__":
    main()
