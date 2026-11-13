from deepagents.backends.utils import create_file_data

from semantic_agent.api.app import uploaded_files, uploaded_text

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
