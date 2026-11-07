from pydantic import SecretStr, model_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore")

    checkpoint_pg_url: SecretStr = SecretStr("")
    pg_pool_max: int = 10
    rust_api_url: str = "http://127.0.0.1:8080"
    rust_timeout_s: dict[str, float] = {"search": 30.0, "judge": 180.0}
    llm_base_url: str = ""
    llm_api_key: SecretStr = SecretStr("")
    llm_model: str = ""
    recursion_limit: int = 60
    ask_timeout_s: float = 300.0
    internal_key: SecretStr = SecretStr("")
    fake_tools: bool = False

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
            if not self.llm_base_url:
                names.append("llm_base_url")
            if not self.llm_model:
                names.append("llm_model")
        return names
