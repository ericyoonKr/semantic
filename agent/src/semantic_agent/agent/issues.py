from collections.abc import Callable
from dataclasses import dataclass, replace
from typing import Any

FACT_STATUSES = frozenset({"사실 있음", "사실 없음", "사실 부족"})


@dataclass(frozen=True)
class Intent:
    subject: str = ""
    act: str = ""
    effect: str = ""
    terms: tuple[str, ...] = ()
    laws: tuple[str, ...] = ()


@dataclass(frozen=True)
class Requirement:
    requirement: str
    facts: str = ""
    refs: tuple[str, ...] = ()
    facts_status: str = ""


@dataclass(frozen=True)
class Issue:
    issue: str = ""
    as_of: str = ""
    version_basis: str = ""
    requirements: tuple[Requirement, ...] = ()
    gaps: tuple[str, ...] = ()
    intent: Intent | None = None


def _text(value: Any) -> str:
    return value.strip() if isinstance(value, str) else ""


def _texts(value: Any) -> tuple[str, ...]:
    if not isinstance(value, list):
        return ()
    return tuple(t for t in (_text(v) for v in value) if t)


def flag(value: Any) -> bool:
    return value is True


def parse_intent(value: Any) -> Intent | None:
    if not isinstance(value, dict):
        return None
    intent = Intent(
        subject=_text(value.get("subject")),
        act=_text(value.get("act")),
        effect=_text(value.get("effect")),
        terms=_texts(value.get("terms")),
        laws=_texts(value.get("laws")),
    )
    return intent if intent != Intent() else None


def _requirement(value: Any) -> Requirement | None:
    if not isinstance(value, dict):
        return None
    name = _text(value.get("requirement"))
    if not name:
        return None
    status = _text(value.get("facts_status"))
    return Requirement(
        requirement=name,
        facts=_text(value.get("facts")),
        refs=_texts(value.get("refs")),
        facts_status=status if status in FACT_STATUSES else "",
    )


def _issue(value: Any) -> Issue | None:
    if not isinstance(value, dict):
        return None
    requirements = value.get("requirements")
    issue = Issue(
        issue=_text(value.get("issue")),
        as_of=_text(value.get("as_of")),
        version_basis=_text(value.get("version_basis")),
        requirements=tuple(
            r for r in (_requirement(v) for v in (requirements if isinstance(requirements, list) else [])) if r
        ),
        gaps=_texts(value.get("gaps")),
        intent=parse_intent(value.get("intent")),
    )
    return issue if issue != Issue() else None


def parse_issues(value: Any) -> tuple[Issue, ...]:
    if not isinstance(value, list):
        return ()
    return tuple(i for i in (_issue(v) for v in value) if i)


def check_refs(issues: tuple[Issue, ...], resolve: Callable[[str], str | None]) -> tuple[Issue, ...]:
    def checked(requirement: Requirement) -> Requirement:
        refs = tuple(r for r in (resolve(ref) for ref in requirement.refs) if r)
        return replace(requirement, refs=refs)

    return tuple(replace(i, requirements=tuple(checked(r) for r in i.requirements)) for i in issues)


def render_issues(issues: tuple[Issue, ...]) -> list[str]:
    if not issues:
        return []
    lines = ["쟁점별 정리:"]
    for issue in issues:
        lines.append(f"- 쟁점: {issue.issue or '적지 않음'}")
        if issue.as_of:
            lines.append(f"  기준 시점: {issue.as_of}")
        if issue.version_basis:
            lines.append(f"  적용 판본: {issue.version_basis}")
        for item in issue.requirements:
            lines.append(f"  - 요건: {item.requirement}")
            if item.facts:
                lines.append(f"    사실: {item.facts}")
            if item.facts_status:
                lines.append(f"    사실 상태: {item.facts_status}")
            lines.append(f"    근거: {', '.join(item.refs) if item.refs else '없음'}")
        lines += [f"  남은 것: {gap}" for gap in issue.gaps]
    return lines
