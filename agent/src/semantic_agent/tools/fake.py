from typing import Any

LAWS = {
    "degraded": False,
    "hits": [
        {
            "law_name": "가짜법",
            "unit_key": "제1조",
            "title": "목적",
            "enforcement_date": "2000-01-01",
            "kind": "가짜판",
            "lsi_seq": "0",
            "text": "이 조문은 가짜 도구가 돌려주는 고정 문장이다.",
            "score": 1.0,
        }
    ],
}

CASES = {
    "degraded": False,
    "hits": [
        {
            "decision_id": "fake-0",
            "court": "가짜법원",
            "case_no": "0000가0000",
            "decided_on": "2000-01-01",
            "field": "가짜분야",
            "section_role": "요지",
            "text": "이 판례 조각은 가짜 도구가 돌려주는 고정 문장이다.",
            "score": 1.0,
        }
    ],
}

LAW_UNIT = {
    "law_name": "가짜법",
    "unit_key": "제1조",
    "title": "목적",
    "body": "이 조문 전문은 가짜 도구가 돌려주는 고정 문장이다.",
    "deleted": False,
    "content_status": "text",
    "source_urls": [],
    "enforcement_date": "2000-01-01",
    "version": {"kind": "가짜판", "lsi_seq": "0", "status": "현행"},
}

CASE = {
    "court": "가짜법원",
    "case_no": "0000가0000",
    "decided_on": "2000-01-01",
    "field": "가짜분야",
    "sections": [{"role": "요지", "heading": "판결요지", "body": "가짜 판결요지 고정 문장."}],
}

CITATIONS = {"items": [{"decision_id": "fake-0", "law": "가짜법", "unit": "제1조"}], "extract_run": "fake-run"}

JUDGE_CATALOG = [
    {
        "claim_type": "가짜청구",
        "node_id": "R1_가짜요건",
        "label": "가짜 요건",
        "gate": "AND",
        "evidence": [{"law": "가짜법", "unit": "제1조"}],
    }
]

KG_CATALOG = [{"graph": "fake-graph", "scope": "가짜 범위", "shape": "찾아보기", "nodes": 1, "status": "가짜"}]

KG_STATE = {"accepted": True, "node": "L1", "name": "가짜 노드", "sha256": "가짜해시", "allowed": ["해당"]}

KG_FIND = {"nodes": [{"node": "L1", "name": "가짜 노드", "kind": "개념", "gloss": "가짜 뜻풀이", "status": "가짜"}]}

KG_EXPAND = {
    "edges": [
        {
            "from": "L1",
            "relation": "가짜관계",
            "certainty": "확실",
            "evidence": "가짜 근거 인용",
            "neighbor": {"node": "L2", "name": "가짜 이웃", "content": "가짜 내용"},
        }
    ]
}

JUDGE = {
    "verdict": {"outcome": "Unknown", "scope": "가짜", "unsat_core": [], "unknown_leaves": [], "assumed": []},
    "text": "가짜 판정 문장: 가짜 도구라 판정하지 않았다.",
    "needed_facts": [{"node_id": "R1_가짜요건", "ask": "가짜 질문: 그 일이 언제 있었나요?"}],
    "path": [{"node_id": "R1_가짜요건", "label": "가짜 요건", "gate": "AND"}],
}

UPLOAD = {
    "file_id": "fake-file",
    "filename": "가짜문서.docx",
    "format": "docx",
    "pages": 1,
    "chars": 20,
    "outline": [{"level": 1, "title": "가짜 제목", "page": 1}],
    "text": "[1쪽]\n가짜 업로드 문서의 고정 문장.",
    "warnings": [],
    "findings": [],
}


def _answer(method: str, path: str) -> Any:
    if method == "DELETE":
        return {}
    if path == "/search/laws":
        return LAWS
    if path == "/search/cases":
        return CASES
    if path == "/laws/unit":
        return LAW_UNIT
    if path.startswith("/cases/"):
        return CASE
    if path == "/citations":
        return CITATIONS
    if path == "/catalog":
        return JUDGE_CATALOG
    if path == "/kg/catalog":
        return KG_CATALOG
    if path.endswith("/kg/find"):
        return KG_FIND
    if path.endswith("/kg/expand"):
        return KG_EXPAND
    if path.endswith("/kg/state-check"):
        return KG_STATE
    if path.endswith("/judge"):
        return JUDGE
    if path.endswith("/files"):
        return UPLOAD
    return {}


class FakeClient:
    def __init__(self) -> None:
        self.calls: list[tuple[str, str]] = []

    async def fetch(
        self,
        method: str,
        path: str,
        *,
        kind: str,
        fail: str,
        json: Any = None,
        params: dict[str, Any] | None = None,
        content: Any = None,
        headers: dict[str, str] | None = None,
    ) -> tuple[Any, str | None]:
        self.calls.append((method, path))
        return _answer(method, path), None

    async def aclose(self) -> None:
        return None
