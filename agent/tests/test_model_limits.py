from pydantic import SecretStr

from semantic_agent.agent.build import build_model
from semantic_agent.config import Settings


def test_model_has_output_cap_and_split_timeouts() -> None:
    settings = Settings(llm_base_url="http://127.0.0.1:1/v1", llm_api_key=SecretStr("x"), llm_model="m")
    model = build_model(settings)
    assert model.max_tokens == 8000
    assert model.max_retries == 1
    assert model.request_timeout.connect == 5.0
    assert model.request_timeout.read == 240.0
