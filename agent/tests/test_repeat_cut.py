import asyncio

import pytest
from langchain_core.language_models.fake_chat_models import GenericFakeChatModel
from langchain_core.messages import AIMessage

from semantic_agent.agent.budget import BudgetExceeded
from semantic_agent.agent.repeat import LIMIT_REPEAT, RepeatCut, repeated


def _stream(text: str) -> str:
    model = GenericFakeChatModel(messages=iter([AIMessage(content=text)]), callbacks=[RepeatCut()])

    async def run() -> str:
        out = ""
        async for chunk in model.astream("질문"):
            out += str(chunk.content)
        return out

    return asyncio.run(run())


def test_looping_answer_is_cut_before_it_ends() -> None:
    loop = "제1조에 따라 처분은 위법하다 " * 200
    with pytest.raises(BudgetExceeded) as exc:
        _stream("처분의 위법성을 봅니다 " + loop)
    assert exc.value.device == LIMIT_REPEAT


def test_normal_answer_and_blank_table_rows_pass() -> None:
    text = " ".join(f"{i}. 제{i}조는 신청인의 사정을 따로 본다" for i in range(1, 80))
    assert _stream(text) == text
    assert not repeated("│" + " " * 38 + "│\n" * 1)
    assert not repeated(("│" + " " * 38 + "│\n") * 20)
    assert repeated(("│" + " " * 38 + "│\n") * 30)
