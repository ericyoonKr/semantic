from contextvars import ContextVar
from dataclasses import dataclass, field
from typing import Any


@dataclass
class Turn:
    tokens: int = 0
    calls: int = 0
    round_tokens: int = 0
    round_calls: int = 0
    rounds: int = 0
    wrap_up: bool = False
    overflow: bool = False
    context_tokens: int = 0
    notes: list[str] = field(default_factory=list)
    guidance: list[str] = field(default_factory=list)
    researcher_first: dict[str, Any] | None = None
    warned: set[str] = field(default_factory=set)
    warnings: list[str] = field(default_factory=list)
    audits: list[dict[str, Any]] = field(default_factory=list)

    def warn_once(self, kind: str) -> None:
        if kind not in self.warned:
            self.warned.add(kind)
            self.warnings.append(kind)

    def audit(self, kind: str, detail: dict[str, Any]) -> None:
        self.audits.append({"kind": kind, "detail": detail})

    def block(self, guard: str, **detail: Any) -> None:
        self.audit("guard_block", {"guard": guard, **detail})

    def drain(self) -> tuple[list[str], list[dict[str, Any]]]:
        warnings, audits = self.warnings, self.audits
        self.warnings, self.audits = [], []
        return warnings, audits


TURN: ContextVar[Turn | None] = ContextVar("turn", default=None)


def current_turn() -> Turn:
    turn = TURN.get()
    if turn is None:
        turn = Turn()
        TURN.set(turn)
    return turn
