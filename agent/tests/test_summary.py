from conftest import ScriptedToolModel
from deepagents.backends import StateBackend

from semantic_agent.agent.build import SUMMARY_KEEP_MESSAGES, SUMMARY_TRIGGER_TOKENS, make_summarization


def test_summarization_replaces_the_default_by_name_and_uses_our_thresholds() -> None:
    middleware = make_summarization(ScriptedToolModel(script=iter([])), StateBackend())
    assert middleware.name == "SummarizationMiddleware"
    assert SUMMARY_TRIGGER_TOKENS < 32_768 and SUMMARY_KEEP_MESSAGES >= 4
