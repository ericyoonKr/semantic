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
