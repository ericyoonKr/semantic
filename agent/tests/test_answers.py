import asyncio

from langchain_core.messages import AIMessage, HumanMessage
from langgraph.store.memory import InMemoryStore

from semantic_agent.api.answers import delete_answers, last_answer_id, rendered, save_answer, saved_answers
from semantic_agent.api.app import history_items
from semantic_agent.api.sse import event


def test_rendered_joins_tokens_and_keeps_last_sources() -> None:
    items = [event("Token", {"text": "가 "}), event("Status", {"tool": "x"}), event("Token", {"text": "나"}), event("Sources", {"items": [{"label": "법 제1조"}]})]
    assert rendered(items) == ("가 나", [{"label": "법 제1조"}])


def test_saved_answer_replaces_raw_text_in_history_and_is_deleted_with_the_session() -> None:
    async def go() -> None:
        store = InMemoryStore()
        messages = [HumanMessage(content="질문", id="h1"), AIMessage(content="원문 [근거 1]", id="a1")]
        assert last_answer_id(messages) == "a1"
        assert await save_answer(store, "u", "s", "a1", [event("Token", {"text": "바꾼 답"}), event("Sources", {"items": [{"label": "법"}]})])
        items = history_items(messages, await saved_answers(store, "u", "s"))
        assert items[-1] == {"role": "assistant", "text": "바꾼 답", "sources": [{"label": "법"}]}
        assert history_items(messages, await saved_answers(store, "other", "s"))[-1]["text"] == "원문 [근거 1]"
        assert await delete_answers(store, "u", "s") == 1
        assert await saved_answers(store, "u", "s") == {}

    asyncio.run(go())


def test_ledger_survives_a_restart_through_the_store() -> None:
    from semantic_agent.api.answers import delete_ledger, load_ledger, save_ledger
    from semantic_agent.ledger import Ledger

    async def go() -> None:
        store = InMemoryStore()
        ledger = Ledger()
        law = ledger.add_law({"law_name": "국가배상법", "unit_key": "제2조", "enforcement_date": "2025-01-07", "body": "① 가\n② 나", "law_id": "1", "version": {"version_id": "v"}})
        ledger.add_case({"court": "대법원", "case_no": "2020두1", "sections": [{"body": "본문"}]}, "77")
        await save_ledger(store, "u", "s", ledger)
        back = await load_ledger(store, "u", "s")
        assert back.get(1) == law and back.knows_case("2020두1")
        assert back.add_law({"law_name": "국가배상법", "unit_key": "제2조", "enforcement_date": "2025-01-07", "body": ""}).ref == 1
        assert back.add_law({"law_name": "행정절차법", "unit_key": "제21조", "body": "x"}).ref == 3
        assert len((await load_ledger(store, "other", "s")).items) == 0
        await delete_ledger(store, "u", "s")
        assert len((await load_ledger(store, "u", "s")).items) == 0

    asyncio.run(go())


def test_ledger_dump_drops_null_characters_postgres_cannot_store() -> None:
    from semantic_agent.ledger import Ledger

    ledger = Ledger()
    ledger.add_law({"law_name": "법", "unit_key": "제1조", "body": "가\x00나"})
    import json

    dumped = json.dumps(ledger.dump(), ensure_ascii=True)
    assert "\\u0000" not in dumped
    assert Ledger.load(json.loads(dumped)).add_law({"law_name": "법", "unit_key": "제1조", "body": "x"}).ref == 1
