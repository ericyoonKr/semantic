import httpx

from semantic_agent.api.sse import model_unreachable


class OpenAIConnectionError(Exception):
    pass


def test_connection_failures_are_told_apart_from_other_errors() -> None:
    assert model_unreachable(OpenAIConnectionError())
    try:
        try:
            raise httpx.ConnectError("x")
        except httpx.ConnectError as inner:
            raise RuntimeError("wrapped") from inner
    except RuntimeError as outer:
        assert model_unreachable(outer)
    assert not model_unreachable(ValueError("x"))
