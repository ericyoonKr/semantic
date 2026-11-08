import asyncio
from typing import Any

from conftest import call, make_agent, run_config, tool_messages
from langchain_core.messages import AIMessage, HumanMessage, SystemMessage

from semantic_agent.agent.progress import PROGRESS_HEAD, fold, progress_text
from semantic_agent.tools.kg import format_edges, format_kg_catalog, format_nodes
from semantic_agent.tools.kg_state import NO_BASIS


def test_missing_state_and_certainty_are_not_printed_as_unknown() -> None:
    text = format_nodes(
        {
            "status": "found",
            "nodes": [
                {"node": "L1", "name": "가", "kind": "concept", "gloss": "뜻"},
                {"node": "L2", "name": "나", "kind": "concept", "gloss": "", "status": None},
                {"node": "L3", "name": "다", "kind": "concept", "status": {"judgment": "not_judged", "reason": "사유"}},
            ],
        }
    )
    lines = text.splitlines()
    assert lines[0] == "상태: 찾음"
    assert lines[1] == "L1 · 가 · concept · 뜻"
    assert lines[2] == "L2 · 나 · concept"
    assert lines[3] == "L3 · 다 · concept · 상태 not_judged (사유)"
    assert "모름" not in text and "None" not in text
    edges = format_edges({"status": "found", "edges": [{"from": "L1", "relation": "is_a", "neighbor": {"node": "L2"}}]})
    assert "L1 -[is_a]-> L2" in edges
    assert "확실성" not in edges and "모름" not in edges


def test_each_of_five_statuses_is_named_with_its_reason() -> None:
    labels = {
        "found": "찾음",
        "none_confirmed": "없음",
        "unknown": "모름",
        "out_of_scope": "범위 밖",
        "not_examined": "아직 안 봄",
    }
    for status, label in labels.items():
        text = format_edges({"status": status, "reason": "이유", "edges": []})
        assert text.splitlines()[0] == f"상태: {label} — 이유"
    text = format_edges(
        {
            "status": "none_confirmed",
            "reason": "이유",
            "basis": ["작업자가 찾아봄"],
            "as_of": {"date": "2024-01-01", "in_force": 0, "not_checked": 2, "excluded": 1},
            "edges": [],
        }
    )
    assert "  찾아본 기록: 작업자가 찾아봄" in text
    assert "기준일 2024-01-01: 효력 있음 0개, 효력 구간이 없어 판단 안 함 2개, 기준일 밖이라 뺌 1개" in text


def test_two_step_results_say_chain_only_and_refused_paths() -> None:
    data = {
        "status": "found",
        "composition_mode": "chain_only",
        "refused_paths": 2,
        "edges": [
            {
                "from": "L1",
                "relation": "part_of",
                "depth": 2,
                "certainty": "confirmed",
                "evidence": [{"kind": "book_evidence", "source": "책"}],
                "checks": {"target_check": {"state": "confirmed"}},
                "neighbor": {"node": "L2", "name": "이웃"},
            }
        ],
    }
    text = format_edges(data)
    assert "끝점 사이의 관계로 줄여 읽지 않는다" in text
    assert "계약에 없는 두 단계 경로 2개는 펼치지 않음" in text
    assert '근거: {"kind": "book_evidence", "source": "책"}' in text
    assert '확인: {"target_check": {"state": "confirmed"}}' in text


def test_catalog_lists_contract_facts_and_disabled_bundles() -> None:
    text = format_kg_catalog(
        {
            "graphs": [
                {
                    "graph": "g",
                    "shape": "연결망",
                    "nodes": 3,
                    "relations": ["is_a"],
                    "compositions": 4,
                    "state": {"kind": "note", "values": ["해당", "해당 아님"]},
                    "dated": 0,
                    "article_index": 0,
                }
            ],
            "disabled": [{"bundle": "b", "reason": "line 2: unknown item"}],
        }
    )
    assert "허용된 두 단계 경로 4개" in text
    assert "적을 수 있는 상태 값 해당, 해당 아님" in text
    assert "조문으로 찾기 없음" in text
    assert "- 읽지 못한 묶음: b (line 2: unknown item)" in text


def test_fold_prefers_last_confirmed_and_drops_retracted() -> None:
    rows = [
        {"graph": "g", "node": "L1", "value": "해당", "status": "confirmed"},
        {"graph": "g", "node": "L1", "value": "해당 아님", "status": "proposed"},
        {"graph": "g", "node": "L2", "value": "해당", "status": "proposed"},
        {"graph": "g", "node": "L2", "value": "해당", "status": "retracted"},
        {"graph": "g", "node": "L3", "value": "참고함", "status": "proposed", "sha256": "old"},
    ]
    folded = fold(rows)
    assert [(r["node"], r["value"]) for r in folded] == [("L1", "해당"), ("L3", "참고함")]
    text = progress_text(rows, {"g": "new"})
    assert "그래프가 바뀜 — 다시 확인" in text.splitlines()[1]


def _system_text(messages: list[Any]) -> str:
    return "\n".join(str(m.content) for m in messages if isinstance(m, SystemMessage))


def test_state_rows_stay_in_this_conversation_checkpoint_only() -> None:
    said = "처분서를 받은 날은 3월 2일입니다"
    script = [
        call("kg_write_state", {"graph": "fake-graph", "node": "L1", "value": "해당", "basis": "근거 없는 말"}, "c1"),
        call("kg_write_state", {"graph": "fake-graph", "node": "L1", "value": "해당", "basis": "처분서를 받은 날"}, "c2"),
        AIMessage(content="적었습니다"),
        AIMessage(content="다음 차례"),
        AIMessage(content="다른 대화"),
    ]

    async def scenario() -> dict[str, Any]:
        agent, model, client = await make_agent(script)
        cfg = run_config()
        first = await agent.ainvoke({"messages": [HumanMessage(content=said)]}, cfg)
        await agent.ainvoke({"messages": [HumanMessage(content="이어서")]}, cfg)
        await agent.ainvoke({"messages": [HumanMessage(content="새 대화")]}, run_config())
        stored = await agent.store.asearch(("user",)) if agent.store else []
        return {"first": first, "seen": model.seen, "calls": client.calls, "stored": stored}

    out = asyncio.run(scenario())
    refused, written = tool_messages(out["first"]["messages"])
    assert refused.content == NO_BASIS
    assert written.content == "적음: L1 가짜 노드 = 해당 (제안)"
    assert [r["value"] for r in out["first"]["kg_progress"]] == ["해당"]
    assert sum(1 for _, path in out["calls"] if path.endswith("/kg/state-check")) == 1
    seen = out["seen"]
    assert PROGRESS_HEAD not in _system_text(seen[1])
    assert "- L1 가짜 노드 (fake-graph): 해당 · 제안 · 근거 「처분서를 받은 날」 · 1번째 질문" in _system_text(seen[2])
    assert PROGRESS_HEAD in _system_text(seen[3])
    assert PROGRESS_HEAD not in _system_text(seen[4])
    assert out["stored"] == []
