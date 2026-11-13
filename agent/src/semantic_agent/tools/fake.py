import re
from typing import Any

import httpx

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

ANNEX = {
    "law_name": "가짜법 시행령",
    "annex_label": "별표 1",
    "title": "가짜 과태료 기준",
    "version_id": "가짜판",
    "valid_from": "2000-01-01",
    "valid_to": None,
    "review_status": "auto",
    "source_urls": [],
}

ANNEX_TABLE = {
    "annex": ANNEX,
    "columns": [["위반행위", "과태료/1차"]],
    "rows": [
        {
            "node_id": "fake-row",
            "path": "a1.t1.가",
            "node_type": "row",
            "label": "가",
            "table": 0,
            "text": "가. 가짜 위반",
            "cells": {"위반행위": {"text": "가. 가짜 위반"}, "과태료/1차": {"text": "1만원"}},
            "amounts": {"과태료/1차": {"kind": "fixed", "krw": 10000}},
            "basis_unit_ids": [],
        }
    ],
    "rows_matched": 1,
    "paras": [],
    "rules": [],
    "related": [],
    "failed_checks": [],
    "notes": ["가짜 도구 결과"],
}

ANNEX_COMPUTE = {
    "annex": ANNEX,
    "node_id": "fake-row",
    "path": "a1.t1.가",
    "label": "가",
    "row_text": "가. 가짜 위반",
    "basis_unit_ids": [],
    "results": [{"column": "과태료/1차", "cell_text": "1만원", "kind": "fixed", "krw": 10000}],
    "paras": [],
    "rules": [],
    "notes": ["가짜 도구 결과"],
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

ORIGINAL = b"%PDF-1.4 fake original"

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


FAKE_READ = re.compile(r"제\s*\d+\s*조|별표\s*\d|서식\s*\d|\d{2,4}[가-힣]{1,2}\d+")
FAKE_KNOWN = ("가짜법 제1조", "0000가0000")


def _fake_check(body: Any) -> dict[str, Any]:
    sentences = body.get("sentences", []) if isinstance(body, dict) else []
    results = []
    for s in sentences:
        rest = s
        for known in FAKE_KNOWN:
            rest = rest.replace(known, "")
        ok = FAKE_READ.search(rest) is None
        results.append({"ok": ok, "reasons": [] if ok else ["unit_not_read"]})
    return {"results": results}


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
    if path == "/annex/lookup":
        return ANNEX_TABLE
    if path == "/annex/compute":
        return ANNEX_COMPUTE
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
        if path == "/answer/check":
            return _fake_check(json), None
        return _answer(method, path), None

    async def open_bytes(self, path: str, *, kind: str) -> httpx.Response | None:
        self.calls.append(("GET", path))
        if not path.endswith(f"/files/{UPLOAD['file_id']}/original"):
            return httpx.Response(404, json={"error": "파일이 없습니다"})
        headers = {"content-type": "application/pdf", "x-file-name": "fake.pdf"}
        return httpx.Response(200, content=ORIGINAL, headers=headers)

    async def aclose(self) -> None:
        return None
