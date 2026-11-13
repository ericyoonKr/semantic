import uuid

from conftest import ScriptedToolModel
from deepagents.backends.utils import create_file_data
from fastapi.testclient import TestClient
from langgraph.checkpoint.memory import InMemorySaver
from langgraph.store.memory import InMemoryStore

from semantic_agent.api.app import create_app, known_upload, uploaded_files, uploaded_text
from semantic_agent.config import Settings
from semantic_agent.tools.fake import ORIGINAL, UPLOAD

KEY = "test-internal-key"


def _app():
    settings = Settings(internal_key=KEY, fake_tools=True)
    return create_app(settings, model=ScriptedToolModel(script=iter([])), saver=InMemorySaver(), store=InMemoryStore())

VALUES = {
    "uploads": [{"file_id": "f1", "filename": "증서.pdf", "format": "pdf", "pages": 2, "uploaded_at": "t", "outline": ["x"]}],
    "files": {"/uploads/f1.md": create_file_data("[1쪽]\n첫 쪽\n[2쪽]\n둘째 쪽")},
}


def test_lists_uploads_without_their_outline() -> None:
    assert uploaded_files(VALUES) == [{"file_id": "f1", "filename": "증서.pdf", "format": "pdf", "pages": 2, "uploaded_at": "t"}]


def test_reads_only_a_known_upload_with_a_safe_id() -> None:
    found = uploaded_text(VALUES, "f1")
    assert found is not None and found["text"].startswith("[1쪽]") and found["filename"] == "증서.pdf"
    assert uploaded_text(VALUES, "f2") is None
    assert uploaded_text(VALUES, "../etc") is None


def test_known_upload_needs_a_safe_listed_id() -> None:
    assert known_upload(VALUES, "f1")
    assert not known_upload(VALUES, "f2")
    assert not known_upload(VALUES, "../f1")


def test_original_bytes_pass_through_only_for_this_sessions_uploads() -> None:
    user = {"x-user-id": str(uuid.uuid4()), "x-internal-key": KEY}
    with TestClient(_app()) as client:
        sid = client.post("/session", headers=user).json()["session_id"]
        assert client.get(f"/session/{sid}/files/{UPLOAD['file_id']}/original", headers=user).status_code == 404
        up = client.post(f"/session/{sid}/upload", headers=user, files={"file": ("a.pdf", b"%PDF-1.4", "application/pdf")})
        assert up.status_code == 200
        got = client.get(f"/session/{sid}/files/{UPLOAD['file_id']}/original", headers=user)
        assert got.status_code == 200
        assert got.content == ORIGINAL
        assert got.headers["content-type"] == "application/pdf"
        assert got.headers["x-file-name"] == "fake.pdf"
        assert client.get(f"/session/{sid}/files/other/original", headers=user).status_code == 404
