import asyncio
import json
from typing import Any

from conftest import call, delegate, make_agent, run_config, tool_messages
from langchain_core.messages import AIMessage, HumanMessage, SystemMessage

from semantic_agent.agent.build import researcher_tools
from semantic_agent.agent.citations import Finding, Findings, check_research
from semantic_agent.agent.delegation import parse_findings, render_findings
from semantic_agent.agent.intent import IntentScope, scope_queries
from semantic_agent.agent.issues import Intent, Issue
from semantic_agent.agent.prompts import RESEARCHER_PROMPT
from semantic_agent.agent.prompts_research import INTENT_NOTE, RESEARCHER_STEPS_PROMPT
from semantic_agent.agent.research import ResearchOptions
from semantic_agent.config import Settings
from semantic_agent.ledger import LEDGER, Ledger
from semantic_agent.tools.fake import LAW_UNIT, FakeClient
from semantic_agent.tools.search import format_laws

LAW_BODY = LAW_UNIT["body"]
READ = {"law": "가짜법", "unit": "제1조"}
KEPT = {"claim": "가짜법 제1조는 목적 조항이다", "source": "근거 1", "quote": LAW_BODY}
INTENT = {"subject": "가짜 영업자", "act": "무허가 영업", "effect": "폐쇄", "terms": ["가짜 용어"], "laws": ["가짜법"]}
ISSUE = {
    "issue": "가짜 영업 폐쇄",
    "as_of": "2020-01-01",
    "version_basis": "현행 판본",
    "intent": INTENT,
    "requirements": [
        {"requirement": "허가 없음", "facts": "허가를 받지 않음", "refs": ["근거 1", "근거 9"], "facts_status": "사실 있음"},
        {"requirement": "영업", "facts": "", "refs": [], "facts_status": "충족"},
    ],
    "gaps": ["근거 못 찾음: 폐쇄 절차"],
}


def steps_answer(found: list[dict[str, Any]], issues: list[Any]) -> AIMessage:
    data = {"issues": issues, "found": found, "not_found": [], "reverse_query": None}
    return AIMessage(content=json.dumps(data, ensure_ascii=False))


def test_switches_are_off_by_default() -> None:
    settings = Settings(_env_file=None)
    options = ResearchOptions.from_settings(settings)
    assert options == ResearchOptions()
    assert options.researcher_prompt() == RESEARCHER_PROMPT
    on = ResearchOptions.from_settings(Settings(_env_file=None, research_prompt="steps", research_intent_scope=True))
    assert on.steps and on.intent_scope and on.researcher_prompt() == RESEARCHER_STEPS_PROMPT


def test_new_fields_are_parsed_and_old_result_still_parses() -> None:
    flagged = {**KEPT, "annex_used": True, "old_law_case": "true"}
    parsed = parse_findings(json.dumps({"issues": [ISSUE, "글자만", {}], "found": [flagged], "not_found": []}, ensure_ascii=False))
    assert parsed is not None
    [item] = parsed.found
    assert item.annex_used is True and item.old_law_case is False
    [issue] = parsed.issues
    assert issue.intent == Intent("가짜 영업자", "무허가 영업", "폐쇄", ("가짜 용어",), ("가짜법",))
    assert [r.facts_status for r in issue.requirements] == ["사실 있음", ""]
    assert issue.gaps == ("근거 못 찾음: 폐쇄 절차",)
    old = parse_findings(json.dumps({"found": [KEPT], "not_found": []}, ensure_ascii=False))
    assert old is not None and old.issues == () and old.found[0] == Finding(KEPT["claim"], "근거 1", LAW_BODY)


def test_check_keeps_quote_rules_and_drops_unknown_requirement_refs() -> None:
    ledger = Ledger()
    ledger.add_law(LAW_UNIT)
    parsed = parse_findings(
        json.dumps(
            {"issues": [ISSUE], "found": [KEPT, {**KEPT, "quote": "지어낸 인용"}, {**KEPT, "source": "근거 9"}], "not_found": []},
            ensure_ascii=False,
        )
    )
    assert parsed is not None
    checked = check_research(parsed, ledger)
    assert [f.source for f in checked.found] == ["[근거 1] 가짜법 제1조(시행 2000-01-01)"]
    assert checked.dropped == 2
    assert checked.issues[0].requirements[0].refs == ("[근거 1] 가짜법 제1조(시행 2000-01-01)",)
    text = render_findings("researcher", checked)
    assert "쟁점별 정리:\n- 쟁점: 가짜 영업 폐쇄\n  기준 시점: 2020-01-01\n  적용 판본: 현행 판본" in text
    assert "    사실 상태: 사실 있음\n    근거: [근거 1] 가짜법 제1조(시행 2000-01-01)" in text
    assert "  - 요건: 영업\n    근거: 없음" in text
    assert "  남은 것: 근거 못 찾음: 폐쇄 절차" in text


def test_old_result_renders_as_before() -> None:
    findings = Findings([Finding("주장", "[근거 1] 가짜법 제1조", "인용")], ["빠진 것"], 0, None)
    text = render_findings("researcher", findings)
    body = text.split("\n", 1)[1].rsplit("\n", 1)[0]
    assert body == "찾은 것:\n- 주장: 주장\n  출처: [근거 1] 가짜법 제1조\n  인용: 인용\n찾지 못한 것:\n- 빠진 것"


def test_law_id_and_matched_units_show_only_in_steps_mode() -> None:
    data = {"hits": [{"law_name": "가짜법", "unit_key": "제1조", "title": "목적", "law_id": "L1", "matched_units": ["1", "1.2"], "text": "본문"}]}
    base, scoped = format_laws(data), format_laws(data, scoped=True)
    assert "법령 ID" not in base and "맞은 항·호" not in base
    assert " · 법령 ID L1]" in scoped and "\n맞은 항·호: 1, 1.2\n본문" in scoped
    base_tools = {t.name: t for t in researcher_tools(FakeClient(), ResearchOptions())}
    steps_tools = {t.name: t for t in researcher_tools(FakeClient(), ResearchOptions(prompt="steps"))}
    assert "law_ids" not in base_tools["search_laws"].args
    assert "law_ids" in steps_tools["search_laws"].args
    assert set(steps_tools) - set(base_tools) == {"resolve_version", "resolve_citations"}


class ScopeClient(FakeClient):
    def __init__(self, hits: list[dict[str, Any]] | None = None) -> None:
        super().__init__()
        self.bodies: list[tuple[str, Any]] = []
        self.hits = hits if hits is not None else [
            {"law_name": "가짜법", "unit_key": "제1조"},
            {"law_name": "가짜법", "unit_key": "제2조"},
        ]

    async def fetch(self, method: str, path: str, **kw: Any) -> tuple[Any, str | None]:
        body = kw.get("json")
        self.bodies.append((path, body))
        if path == "/laws/scope":
            self.calls.append((method, path))
            return {"law_ids": ["L9"]}, None
        if path == "/search/laws" and isinstance(body, dict) and body.get("law_ids"):
            self.calls.append((method, path))
            return {"hits": self.hits}, None
        return await super().fetch(method, path, **kw)


def test_scope_search_uses_law_ids_and_both_queries_and_skips_read_units() -> None:
    client = ScopeClient()
    ledger = Ledger()
    ledger.add_law(LAW_UNIT)
    issue = Issue(issue="가짜 영업 폐쇄", as_of="불명", intent=Intent(act="무허가 영업", terms=("가짜 용어",), laws=("가짜법",)))
    picks = asyncio.run(IntentScope(client).unread("사안의 원래 말", [issue, Issue(issue="법령 없음", intent=Intent(act="x"))], ledger))
    assert picks == ["가짜법 제2조"]
    assert client.bodies[0] == ("/laws/scope", {"names": ["가짜법"]})
    searched = [b for p, b in client.bodies if p == "/search/laws"]
    assert [b["query"] for b in searched] == ["사안의 원래 말", "가짜 용어 무허가 영업"]
    assert all(b["law_ids"] == ["L9"] and b["as_of"] is None for b in searched)
    assert scope_queries("같은 말", Intent(act="같은 말")) == ["같은 말"]


def _run(script: list[AIMessage], client: FakeClient, research: ResearchOptions | None) -> tuple[list[Any], Any]:
    async def scenario() -> tuple[list[Any], Any]:
        LEDGER.set(Ledger())
        agent, model, _ = await make_agent(script, client=client, research=research)
        result = await agent.ainvoke({"messages": [HumanMessage(content="가짜 영업 폐쇄")]}, run_config())
        return result["messages"], model

    return asyncio.run(scenario())


def test_intent_scope_reruns_researcher_once_with_unread_articles() -> None:
    client = ScopeClient()
    script = [
        delegate("researcher", "쟁점: 가짜 영업 폐쇄\n목적: 조회", "t1"),
        call("read_law", READ, "c1"),
        steps_answer([KEPT], [ISSUE]),
        steps_answer([KEPT], [ISSUE]),
        AIMessage(content="끝"),
    ]
    messages, model = _run(script, client, ResearchOptions(prompt="steps", intent_scope=True))
    system = [m for m in model.seen[1] if isinstance(m, SystemMessage)][0]
    assert "검색어 바꾸기 예시" in system.content
    rerun = [m for m in model.seen[3] if isinstance(m, HumanMessage)][-1]
    assert rerun.content.endswith(f"먼저 읽을 조문: 가짜법 제2조\n\n{INTENT_NOTE}")
    assert [c for c in client.calls if c[1] == "/laws/scope"] == [("POST", "/laws/scope")]
    result = tool_messages(messages)[-1].content
    assert KEPT["claim"] in result and "쟁점별 정리:" in result
    assert model.calls == 5


def test_intent_scope_off_makes_no_scope_calls() -> None:
    client = ScopeClient()
    script = [
        delegate("researcher", "쟁점: 가짜 영업 폐쇄\n목적: 조회", "t1"),
        call("read_law", READ, "c1"),
        steps_answer([KEPT], [ISSUE]),
        AIMessage(content="끝"),
    ]
    _, model = _run(script, client, None)
    assert client.calls == [("GET", "/laws/unit")]
    system = [m for m in model.seen[1] if isinstance(m, SystemMessage)][0]
    assert RESEARCHER_PROMPT in system.content and "검색어 바꾸기 예시" not in system.content
    assert model.calls == 4


def test_intent_scope_without_unread_articles_returns_first_result() -> None:
    client = ScopeClient(hits=[{"law_name": "가짜법", "unit_key": "제1조"}])
    script = [
        delegate("researcher", "쟁점: 가짜 영업 폐쇄\n목적: 조회", "t1"),
        call("read_law", READ, "c1"),
        steps_answer([KEPT], [ISSUE]),
        AIMessage(content="끝"),
    ]
    _, model = _run(script, client, ResearchOptions(intent_scope=True))
    assert ("POST", "/laws/scope") in client.calls
    assert model.calls == 4
