from semantic_agent.api.thought import ThoughtFilter


def _run(chunks: list[str]) -> str:
    f = ThoughtFilter()
    return "".join(f.push(c) for c in chunks) + f.flush()


def test_gemma_thought_channel_is_removed_even_when_split_across_chunks() -> None:
    assert _run(["<|channel>thought\n속으로 생각<channel|>답입니다."]) == "답입니다."
    assert _run(["<|chan", "nel>thought\n생", "각<chan", "nel|>", "답입니다."]) == "답입니다."
    assert _run(["앞 <think>x</think>뒤"]) == "앞 뒤"


def test_plain_text_and_lone_markers() -> None:
    assert _run(["그냥 ", "답입니다 <", "3이 아님"]) == "그냥 답입니다 <3이 아님"
    assert _run(["답<channel|>입니다"]) == "답입니다"
    assert _run(["<|channel>thought\n끝나지 않은 생각"]) == ""
