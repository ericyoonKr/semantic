import uuid

from conftest import ScriptedToolModel
from fastapi.testclient import TestClient
from langgraph.checkpoint.memory import InMemorySaver
from langgraph.store.memory import InMemoryStore

from semantic_agent.api.app import create_app
from semantic_agent.config import Settings

KEY = "test-internal-key"


def _app():
    settings = Settings(internal_key=KEY, fake_tools=True)
    model = ScriptedToolModel(script=iter([]))
    return create_app(settings, model=model, saver=InMemorySaver(), store=InMemoryStore())


def test_request_without_internal_key_is_rejected() -> None:
    user = {"x-user-id": str(uuid.uuid4())}
    with TestClient(_app()) as client:
        assert client.post("/session", headers=user).status_code == 403
        assert client.post("/session", headers={**user, "x-internal-key": "wrong"}).status_code == 403
        ok = client.post("/session", headers={**user, "x-internal-key": KEY})
        assert ok.status_code == 200
        assert uuid.UUID(ok.json()["session_id"])


def test_retry_replaces_the_last_answer_and_refuses_when_nothing_was_asked() -> None:
    from langchain_core.messages import AIMessage

    settings = Settings(internal_key=KEY, fake_tools=True)
    model = ScriptedToolModel(script=iter([AIMessage(content="첫 답입니다."), AIMessage(content="다시 쓴 답입니다.")]))
    app = create_app(settings, model=model, saver=InMemorySaver(), store=InMemoryStore())
    headers = {"x-user-id": str(uuid.uuid4()), "x-internal-key": KEY}
    with TestClient(app) as client:
        sid = client.post("/session", headers=headers).json()["session_id"]
        assert client.post(f"/session/{sid}/retry", headers=headers).status_code == 404
        first = client.post(f"/session/{sid}/ask", json={"question": "질"}, headers=headers)
        assert first.status_code == 200 and "첫 답입니다" in first.text
        again = client.post(f"/session/{sid}/retry", headers=headers)
        assert again.status_code == 200 and "다시 쓴 답입니다" in again.text and "첫 답입니다" not in again.text
        history = client.get(f"/session/{sid}/history", headers=headers).json()["messages"]
        assert [m["role"] for m in history] == ["user", "assistant"]
        assert history[0]["text"] == "질" and history[1]["text"].startswith("다시 쓴 답입니다")
