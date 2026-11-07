import asyncio

from conftest import answer, call, delegate, make_agent, run_config, tool_messages
from langchain_core.messages import AIMessage, HumanMessage

from semantic_agent.agent.authority import LEDGER, Ledger
from semantic_agent.agent.citations import Finding, Findings, check_document, check_graph
from semantic_agent.api.app import record_upload

FILE_A = "/uploads/aaa.md"
FILE_B = "/uploads/bbb.md"
TEXT_A = "[1쪽]\n제3조(계약기간) 이 계약의 기간은 2024. 1. 1.부터 1년으로 한다.\n제4조(대금) 갑은 을에게 매월\n말일에 대금을 지급한다.\n[2쪽]\n제5조(해지) 을이 의무를 위반하면 갑은 계약을 해지할 수 있다."
TEXT_B = "[1쪽]\n제1조(목적) 이 약정은 용역의 범위를 정한다."
FILES = {FILE_A: TEXT_A, FILE_B: TEXT_B}

DOC_GOOD = [
    (f"{FILE_A} · 1쪽", "이 계약의 기간은 2024. 1. 1.부터 1년으로 한다."),
    (f"{FILE_A} · 1쪽", "갑은 을에게 매월 말일에 대금을 지급한다."),
    (f"{FILE_A} · 1쪽", "갑은 을에게 매월\n말일에 대금을 지급한다."),
    (f"{FILE_A} · 1쪽", "갑은  을에게   매월 말일에 대금을 지급한다."),
    (f"{FILE_A} · 1쪽", "계 약의 기간은 2024 . 1. 1.부터"),
    (f"{FILE_A} · 2쪽", "을이 의무를 위반하면 갑은 계약을 해지할 수 있다."),
    (f"{FILE_A}, 2쪽", "제5조(해지)"),
    (f"파일 {FILE_A} 2쪽", "갑은 계약을 해지할 수 있다"),
    (f"{FILE_B} · 1쪽", "이 약정은 용역의 범위를 정한다."),
    (f"{FILE_B}·1쪽", "용역의 범위"),
]
DOC_BAD = [
    (f"{FILE_A} · 1쪽", "이 계약의 기간은 2024. 1. 1.부터 2년으로 한다."),
    (f"{FILE_A} · 1쪽", "갑은 을에게 매주 말일에 대금을 지급한다."),
    (f"{FILE_A} · 2쪽", "을이 의무를 위반하면 갑은 손해배상을 청구할 수 있다."),
    (f"{FILE_B} · 1쪽", "이 계약의 기간은 2024. 1. 1.부터 1년으로 한다."),
    ("/uploads/ccc.md · 1쪽", "이 약정은 용역의 범위를 정한다."),
    ("계약서.docx · 1쪽", "이 계약의 기간은 2024. 1. 1.부터 1년으로 한다."),
    ("1쪽", "이 계약의 기간은 2024. 1. 1.부터 1년으로 한다."),
    (f"{FILE_A} · 1쪽", ""),
    (f"{FILE_A} · 1쪽", "   "),
    (f"{FILE_A} · 2쪽", "을이 의무를 위반하더라도 갑은 계약을 해지할 수 없다."),
]

KG_NODES = {"L1", "L2", "L7"}
KG_GOOD = ["fake-graph · L1", "fake-graph · L2", "fake-graph · L1, L2", "L7", "그래프 fake-graph 노드 L7", "fake-graph · L1 → L2"]
KG_BAD = ["fake-graph · L9", "fake-graph", "fake-graph · L1, L8", "fake-graph · L10", "fake-graph · LL1", "fake-graph · 노드 1", "", "fake-graph · L1x"]


def _findings(pairs: list[tuple[str, str]]) -> Findings:
    return Findings([Finding(f"주장 {i}", source, quote) for i, (source, quote) in enumerate(pairs)], [], 0, None)


def test_document_check_accuracy() -> None:
    kept = check_document(_findings(DOC_GOOD), lambda path: FILES.get(path, ""))
    dropped = check_document(_findings(DOC_BAD), lambda path: FILES.get(path, ""))
    assert len(kept.found) == len(DOC_GOOD) == 10
    assert dropped.found == [] and dropped.dropped == len(DOC_BAD) == 10


def test_graph_check_accuracy() -> None:
    ledger = Ledger()
    ledger.record_kg_nodes(KG_NODES)
    kept = check_graph(_findings([(s, "근거") for s in KG_GOOD]), ledger)
    dropped = check_graph(_findings([(s, "근거") for s in KG_BAD]), ledger)
    assert len(kept.found) == len(KG_GOOD) == 6
    assert dropped.found == [] and dropped.dropped == len(KG_BAD) == 8


async def _ask_with_upload(script: list[AIMessage]) -> list:
    LEDGER.set(Ledger())
    agent, _, _ = await make_agent(script)
    cfg = run_config()
    await record_upload(agent, cfg, {"file_id": "aaa", "filename": "계약서.docx", "pages": 2, "text": TEXT_A})
    result = await agent.ainvoke({"messages": [HumanMessage(content="계약 기간이 얼마야?")]}, cfg)
    return result["messages"]


def test_doc_reader_lines_must_quote_the_uploaded_file() -> None:
    script = [
        delegate("doc_reader", "계약 기간을 찾아 줘", "t1"),
        answer(
            [
                {"claim": "기간은 1년", "source": f"{FILE_A} · 1쪽", "quote": "이 계약의 기간은 2024. 1. 1.부터 1년으로 한다."},
                {"claim": "기간은 2년", "source": f"{FILE_A} · 1쪽", "quote": "이 계약의 기간은 2년으로 한다."},
                {"claim": "다른 파일", "source": f"{FILE_B} · 1쪽", "quote": "이 계약의 기간은 2024. 1. 1.부터 1년으로 한다."},
            ]
        ),
        AIMessage(content="끝"),
    ]
    result = tool_messages(asyncio.run(_ask_with_upload(script)))[-1].content
    assert "기간은 1년" in result
    assert "기간은 2년" not in result
    assert "다른 파일" not in result


def test_kg_explorer_lines_must_name_returned_nodes() -> None:
    script = [
        delegate("kg_explorer", "가짜 개념의 관계를 따라가 줘", "t1"),
        call("kg_find", {"graph": "fake-graph", "text": "가짜"}, "c1"),
        call("kg_expand", {"graph": "fake-graph", "node": "L1", "direction": "out"}, "c2"),
        answer(
            [
                {"claim": "찾은 노드", "source": "fake-graph · L1", "quote": "가짜 뜻풀이"},
                {"claim": "이웃 노드", "source": "fake-graph · L2", "quote": "가짜 내용"},
                {"claim": "지어낸 노드", "source": "fake-graph · L5", "quote": "가짜 내용"},
            ]
        ),
        AIMessage(content="끝"),
    ]

    async def scenario() -> list:
        LEDGER.set(Ledger())
        agent, _, _ = await make_agent(script)
        result = await agent.ainvoke({"messages": [HumanMessage(content="관계를 알려 줘")]}, run_config())
        return result["messages"]

    result = tool_messages(asyncio.run(scenario()))[-1].content
    assert "찾은 노드" in result and "이웃 노드" in result
    assert "지어낸 노드" not in result
