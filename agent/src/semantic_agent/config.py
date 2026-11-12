from typing import Literal

from pydantic import Field, SecretStr, model_validator
from pydantic_settings import BaseSettings, SettingsConfigDict

GEMINI_PROVIDERS = frozenset({"gemini", "vertex"})


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore")

    checkpoint_pg_url: SecretStr = SecretStr("")
    pg_pool_max: int = 10
    rust_api_url: str = "http://127.0.0.1:8080"
    rust_timeout_s: dict[str, float] = {"search": 30.0, "judge": 180.0}
    llm_provider: Literal["openai", "gemini", "vertex"] = "openai"
    llm_base_url: str = ""
    llm_api_key: SecretStr = SecretStr("")
    llm_model: str = ""
    llm_max_tokens: int = 8000
    llm_connect_timeout_s: float = 5.0
    llm_read_timeout_s: float = 240.0
    llm_max_retries: int = 1
    vertex_api_key: SecretStr = SecretStr("")
    vertex_model: str = ""
    vertex_thinking_level: Literal["minimal", "low", "medium", "high"] | None = None
    vertex_thinking_budget: int | None = None
    vertex_max_concurrency: int = Field(default=4, ge=1)
    recursion_limit: int = 60
    ask_timeout_s: float = 300.0
    internal_key: SecretStr = SecretStr("")
    fake_tools: bool = False
    data_dir: str = ""

    @model_validator(mode="after")
    def _deadlines_nest(self) -> "Settings":
        search = self.rust_timeout_s.get("search")
        judge = self.rust_timeout_s.get("judge")
        if search is None or judge is None:
            raise ValueError("rust_timeout_s에 search와 judge가 모두 있어야 합니다")
        if not self.ask_timeout_s > judge > search > 0:
            raise ValueError("마감 순서가 맞지 않습니다: ask_timeout_s > judge > search 여야 합니다")
        return self

    def missing(self) -> list[str]:
        names = []
        if not self.internal_key.get_secret_value():
            names.append("internal_key")
        if not self.fake_tools:
            if not self.checkpoint_pg_url.get_secret_value():
                names.append("checkpoint_pg_url")
            names.extend(self._missing_model())
        return names

    def _missing_model(self) -> list[str]:
        if self.llm_provider in GEMINI_PROVIDERS:
            needed = {"vertex_api_key": self.vertex_api_key.get_secret_value(), "vertex_model": self.vertex_model}
        else:
            needed = {"llm_base_url": self.llm_base_url, "llm_model": self.llm_model}
        return [name for name, value in needed.items() if not value]
