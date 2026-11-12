from dataclasses import dataclass
from typing import Literal

from semantic_agent.agent.prompts import RESEARCHER_PROMPT
from semantic_agent.agent.prompts_research import RESEARCHER_STEPS_PROMPT
from semantic_agent.config import Settings

ResearchPrompt = Literal["base", "steps"]
STEPS: ResearchPrompt = "steps"
VERSION_TOOLS = ("resolve_version", "resolve_citations")


@dataclass(frozen=True)
class ResearchOptions:
    prompt: ResearchPrompt = "base"
    intent_scope: bool = False

    @classmethod
    def from_settings(cls, settings: Settings) -> "ResearchOptions":
        return cls(prompt=settings.research_prompt, intent_scope=settings.research_intent_scope)

    @property
    def steps(self) -> bool:
        return self.prompt == STEPS

    def researcher_prompt(self) -> str:
        return RESEARCHER_STEPS_PROMPT if self.steps else RESEARCHER_PROMPT
