from langchain_core.messages import AIMessage, HumanMessage, ToolMessage

from semantic_agent.api.retry import retry_cut
from semantic_agent.tools.judge import UPLOAD_KIND


def test_retry_cut_finds_the_last_question_and_drops_what_followed_except_upload_marks() -> None:
    marker = HumanMessage(content="[업로드] a.pdf, 3쪽", additional_kwargs={"kind": UPLOAD_KIND}, id="u1")
    summary = HumanMessage(content="앞선 대화 요약", additional_kwargs={"lc_source": "summarization"}, id="s1")
    messages = [
        summary,
        HumanMessage(content="첫 질문", id="h1"),
        AIMessage(content="첫 답", id="a1"),
        HumanMessage(content="둘째 질문", id="h2"),
        AIMessage(content="", tool_calls=[{"name": "task", "args": {}, "id": "c1"}], id="a2"),
        ToolMessage(content="결과", tool_call_id="c1", id="t1"),
        marker,
        AIMessage(content="둘째 답", id="a3"),
    ]
    question, dropped = retry_cut(messages)
    assert question == "둘째 질문"
    assert [m.id for m in dropped] == ["h2", "a2", "t1", "a3"]
    assert retry_cut([summary, marker]) == (None, [])
    assert retry_cut([]) == (None, [])
