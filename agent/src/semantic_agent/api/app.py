import asyncio
import secrets
import uuid
from contextlib import asynccontextmanager
from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import Any

import uvicorn
from deepagents.backends.utils import create_file_data
from fastapi import Depends, FastAPI, Header, HTTPException, Request, Response
from fastapi.responses import JSONResponse, StreamingResponse
from langchain_core.language_models import BaseChatModel
from langchain_core.messages import AIMessage, HumanMessage, ToolMessage
from langgraph.checkpoint.base import BaseCheckpointSaver
from langgraph.store.base import BaseStore
from pydantic import BaseModel, Field

from semantic_agent.agent.build import build_agent, load_catalogs
from semantic_agent.agent.research import ResearchOptions
from semantic_agent.agent.model import build_model
from semantic_agent.api.sessions import SessionRegistry, run_ask, run_upload
from semantic_agent.api.runlog import RunLog
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


def history_items(messages: list[Any]) -> list[dict[str, str]]:
    items: list[dict[str, str]] = []
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
            items.append({"role": "assistant", "text": message.text})
    return items


def upload_line(data: dict[str, Any]) -> str:
    line = f"[업로드] {data.get('filename', '')}, {data.get('pages', 0)}쪽"
    warnings = [str(w) for w in data.get("warnings") or []]
    return line + (f", 추출 불가: {'; '.join(warnings)}" if warnings else "")


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
        }
    )
    marker = HumanMessage(content=upload_line(data), additional_kwargs={"kind": UPLOAD_KIND})
    await agent.aupdate_state(
        cfg,
        {"files": {path: create_file_data(str(data.get("text", "")))}, "uploads": uploads, "messages": [marker]},
        as_node="model",
    )


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
            try:
                async for item in to_sse(state.agent, body.question, cfg, settings.ask_timeout_s):
                    log.add(item)
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
        snapshot = await rt(request).agent.aget_state(_config(settings, sid, user_id))
        return {"messages": history_items(list(snapshot.values.get("messages", [])))}

    @app.delete("/session/{session_id}", status_code=204)
    async def remove_session(session_id: str, request: Request, user_id: str = Depends(user_id_header)) -> Response:
        sid = _session(session_id)
        state = rt(request)
        await delete_session(state.saver, sid)
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
