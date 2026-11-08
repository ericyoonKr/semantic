import asyncio
import json

import httpx

from semantic_agent.config import Settings
from semantic_agent.tools.annex import format_computation, format_table, make_annex_tools
from semantic_agent.tools.client import RustClient
from semantic_agent.tools.search import format_law_unit, format_laws

ANNEX = {
    "law_name": "산업기술법 시행령",
    "annex_label": "별표 2",
    "title": "과태료의 부과기준(제37조 관련)",
    "title_related": "(제37조 관련)",
    "version_id": "X|별표|2|0@2025-10-01",
    "valid_from": "2025-10-01",
    "valid_to": None,
    "review_status": "auto",
    "source_urls": ["https://example.invalid/a.pdf"],
}

TABLE = {
    "annex": ANNEX,
    "columns": [["위반행위", "근거 법조문", "과태료/1차 위반", "과태료/2차 이상 위반"]],
    "rows": [
        {
            "node_id": "n-나",
            "path": "a2.개별기준.t1.나",
            "node_type": "row",
            "label": "나",
            "table": 0,
            "text": "나. 등록을 신청하지 않은 경우",
            "cells": {
                "과태료/2차 이상 위반": {"text": "700"},
                "근거 법조문": {"text": "법 제39조제1항제2호"},
                "위반행위": {"text": "나. 등록을 신청하지 않은 경우"},
                "과태료/1차 위반": {"text": "500"},
            },
            "amounts": {
                "과태료/1차 위반": {"kind": "fixed", "krw": 5000000},
                "과태료/2차 이상 위반": {"kind": "list", "items": [{"kind": "fixed", "krw": 7000000}, {"kind": "text", "source": "영업정지"}]},
            },
            "basis_unit_ids": [],
        }
    ],
    "rows_matched": 1,
    "paras": [{"node_id": "p1", "path": "a2.일반기준.p1_가", "node_type": "rule_para", "label": "가", "text": "가. 위반행위의 횟수에 따른 기준은 최근 1년간"}],
    "rules": [],
    "related": [{"law_name": "산업기술법 시행령", "article_no": "제37조", "ref_type": "as_in", "ref_text": "별표 2", "version_id": "v"}],
    "failed_checks": ["t1 columns_aligned (misaligned_lines=2)"],
    "notes": ["검수 전 판본(review_status=auto) — 금액·기준은 원문 확인 필요"],
}


def test_table_lists_cells_in_column_order_with_amounts_rules_and_warnings() -> None:
    text = format_table(TABLE)
    order = [text.index(k) for k in ("위반행위: 나.", "근거 법조문: 법", "과태료/1차 위반: 500", "과태료/2차 이상 위반: 700")]
    assert order == sorted(order)
    assert "금액 과태료/1차 위반: 5,000,000원" in text
    assert "금액 과태료/2차 이상 위반: 7,000,000원 / 금액으로 읽지 못함: 영업정지" in text
    assert "node_id n-나" in text
    assert "주의: 검수 전 판본" in text
    assert "표 복원 검사 실패: t1 columns_aligned" in text
    assert "일반기준·비고(함께 제시):\n- 가 가. 위반행위의 횟수" in text
    assert "관련 조문: 산업기술법 시행령 제37조(as_in)" in text
    assert "원본: https://example.invalid/a.pdf" in text
    assert text.rstrip().endswith("숫자를 쓰지 않는다.")


def test_computation_shows_value_cell_text_work_and_unapplied_rules() -> None:
    data = {
        "annex": ANNEX,
        "node_id": "n",
        "path": "a2.t1.가",
        "label": "가",
        "row_text": "가. 지연 신고",
        "results": [
            {"column": "과태료", "cell_text": "2만원에 91일째부터 …", "kind": "formula", "krw": 120000,
             "work": "20000원 + 10000원 × 10회 = 120000원", "source": "2만원에 91일째부터 3일 초과 시마다 1만원을 더한 금액"},
            {"column": "과태료", "item": 2, "cell_text": "x", "kind": "text", "krw": None, "note": "금액으로 읽지 못한 칸 — 원문 그대로"},
        ],
        "paras": [{"label": None, "text": "감경 문단"}],
        "notes": ["일반기준(가중·감경·상한)은 적용하지 않은 값 — paras를 함께 제시"],
    }
    text = format_computation(data)
    assert "- 과태료: 120,000원 (칸 원문 「2만원에 91일째부터 …」) 계산 20000원 + 10000원 × 10회 = 120000원" in text
    assert "산식 원문 「2만원에 91일째부터 3일 초과 시마다 1만원을 더한 금액」" in text
    assert "- 과태료 값2: 값 없음" in text
    assert "일반기준·비고(적용하지 않음, 함께 제시):\n- 감경 문단" in text
    assert format_computation({"annex": ANNEX, "results": []}).count("계산한 금액 없음") == 1


def test_law_formatting_shows_degraded_reason_version_and_scheduled_date() -> None:
    laws = format_laws(
        {
            "degraded": True,
            "degraded_reason": "벡터 칸이 비어 있어 글자 검색만 함",
            "hits": [
                {"law_name": "지방공무원법", "unit_key": "제25조의2", "title": "임용", "enforcement_date": "2021-12-09",
                 "version_id": "001653:0025021@2021-12-09", "scheduled_from": "2027-01-01", "text": "본문"}
            ],
        }
    )
    assert laws.startswith("강등: 벡터 칸이 비어 있어 글자 검색만 함")
    assert "· 001653:0025021@2021-12-09 · 시행 예정: 2027-01-01]" in laws
    unit = format_law_unit(
        {"law_name": "지방공무원법", "unit_key": "제1조", "title": "목적", "enforcement_date": "2021-12-09",
         "version": {"version_id": "v1", "status": "effective"}, "body": "본문", "content_status": "text"}
    )
    assert unit.splitlines()[0] == "[지방공무원법 제1조(목적) · 2021-12-09 · v1 effective]"


def test_tools_send_the_request_body_and_turn_errors_into_failure_text() -> None:
    seen: list[httpx.Request] = []

    def handle(request: httpx.Request) -> httpx.Response:
        seen.append(request)
        if request.url.path == "/annex/lookup":
            return httpx.Response(200, json=TABLE)
        return httpx.Response(400, json={"error": "그 별표 행을 찾지 못했습니다"})

    settings = Settings(internal_key="k", rust_api_url="http://rust.test", fake_tools=False)
    tools = {t.name: t for t in make_annex_tools(RustClient(settings, transport=httpx.MockTransport(handle)))}

    async def scenario() -> tuple[str, str]:
        table = await tools["annex_lookup"].ainvoke({"law": "산업기술법 시행령", "annex": "별표 2", "words": "등록"})
        out = await tools["annex_compute"].ainvoke({"node_id": "없음", "elapsed": 120})
        return table, out

    table, out = asyncio.run(scenario())
    assert json.loads(seen[0].content) == {
        "law": "산업기술법 시행령", "annex": "별표 2", "as_of": None, "row": None,
        "words": "등록", "column": None, "limit": 50,
    }
    assert json.loads(seen[1].content) == {"node_id": "없음", "column": None, "elapsed": 120}
    assert table.startswith('<자료 출처="산업기술법 시행령 별표 2">')
    assert out.startswith("검색 실패")
