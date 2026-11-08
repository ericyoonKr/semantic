import json
from typing import Any, Literal

from langchain_core.tools import BaseTool, tool
from pydantic import BaseModel

from semantic_agent.tools.client import SEARCH_FAIL, IsoDate, ToolClient, material

CaseKind = Literal[
    "CriminalSubstantive",
    "CriminalProcedure",
    "AdminSanction",
    "AdminFine",
    "AdminApplication",
    "AdminGeneral",
    "AdminLitigation",
    "AdminOmission",
    "CivilSubstantive",
    "CivilProcedure",
    "Tax",
    "InternationalInheritance",
]
EventKind = Literal[
    "Act",
    "ApplicationFiled",
    "Disposition",
    "ContractConcluded",
    "Tort",
    "TaxPeriod",
    "TaxLiability",
    "IncomeAccrued",
    "ProceedingPending",
    "PaymentDue",
    "ProcedureAct",
    "Completed",
    "Death",
    "Indictment",
    "FirstInstanceClosed",
    "FactualTrialClosed",
    "Judgment",
    "Finalized",
    "UnconstitutionalDecision",
    "PriorConstitutionalDecision",
    "PrecedentChange",
    "NonconformityDecision",
]

KIND_HELP = (
    "kind: CriminalSubstantive 형사 실체법, CriminalProcedure 형사 절차법, AdminSanction 행정 제재처분, "
    "AdminFine 과태료, AdminApplication 신청에 따른 처분, AdminGeneral 행정 일반, AdminLitigation 항고소송 위법 판단, "
    "AdminOmission 부작위위법확인, CivilSubstantive 민사 실체법, CivilProcedure 민사 절차법, Tax 세법, "
    "InternationalInheritance 국제 상속."
)
FACT_HELP = (
    "facts: 사건마다 event와 날짜. 하루면 on, 날짜를 정확히 모르면 start·end(예: 「2020. 1.경」은 2020-01-01~2020-01-31), "
    "실제로 계속된 기간이면 start·end와 continuing=true. event: Act 행위, ApplicationFiled 신청, Disposition 처분, "
    "ContractConcluded 계약 체결, Tort 불법행위, TaxPeriod 과세기간, TaxLiability 납세의무 성립, IncomeAccrued 소득 귀속, "
    "ProceedingPending 절차 계속, PaymentDue 지급사유, ProcedureAct 절차 진행, Completed 완성, Death 사망, Indictment 공소제기, "
    "FirstInstanceClosed 1심 변론종결, FactualTrialClosed 사실심 변론종결, Judgment 판결 선고, Finalized 확정, "
    "UnconstitutionalDecision 위헌결정, PriorConstitutionalDecision 종전 합헌결정, PrecedentChange 판례 변경, "
    "NonconformityDecision 헌법불합치결정. 날짜는 사용자 진술이나 문서에 있는 것만 넣고 추측하지 않는다."
)

STATUS_LABELS = {
    "Decided": "결정됨",
    "MissingFacts": "사실이 모자라 되물어야 함",
    "Undetermined": "판정 보류",
}
NOT_JUDGED = {
    "NoPrinciple": "이 분야에 기준시 원칙이 없음",
    "NoRegisteredTime": "원칙의 기준 시점이 등록된 사실과 이어지지 않음",
    "SeveralTimes": "원칙의 기준 시점이 둘 이상",
}
FLAG_LABELS = {
    "Boundary": "경계일",
    "UnreviewedRule": "미검수 부칙 규칙에 기댐",
    "FullRevisionRetired": "전부개정으로 실효된 부칙",
    "FullRevisionKept": "전부개정 뒤에도 유지 표시된 부칙",
    "UnconstitutionalScope": "위헌결정 소급 범위 확인",
    "Review": "검토",
    "RefTimeUndetermined": "기준시 미확정",
    "AddendumUndetermined": "부칙 미평가(판정 보류)",
    "NoVersion": "판본 없음",
    "BasisUnverified": "근거 원문 미확인",
    "DataHold": "자료 부족(판정 보류)",
    "DataReview": "자료 검토",
}
MISSING_LABELS = {
    "Absent": "없음",
    "Several": "같은 종류가 여럿",
    "Imprecise": "날짜가 구간이라 정하지 못함",
    "Conflicting": "서로 걸리는 사실이 함께 있음",
}
CITE_FLAGS = {
    "SeveralVersionsInSpan": "인용 구간 안에 판본이 여럿",
    "AssumedCurrent": "무표시 인용이라 선고일 현행으로 봄",
    "Remapped": "조문 이동으로 다른 조문 키로 바꿈",
    "ListNotInterpreted": "범위·대용 표현을 읽지 않음",
    "Data": "자료 검토",
}
LANE_LABELS = {
    "Same": "일치",
    "AmongSeveral": "예측 판본 여럿 가운데 있음",
    "Different": "불일치",
    "NoAnswer": "답변 줄 없음",
    "NoPrediction": "예측 판본 없음",
}
EXPERT = "전문가 확인 필요"
BODY_CHARS = 300
BODY_NOTE = "본문은 발췌라 인용할 수 없음 — 인용하려면 read_law로 그 시작일 판본을 읽는다"


class FactArg(BaseModel):
    event: EventKind
    on: IsoDate | None = None
    start: IsoDate | None = None
    end: IsoDate | None = None
    continuing: bool = False


def fact_body(facts: list[FactArg] | None) -> list[dict[str, Any]]:
    return [
        {"event": f.event, "on": f.on, "from": f.start, "to": f.end, "continuing": f.continuing}
        for f in facts or []
    ]


def _compact(value: Any) -> str:
    if isinstance(value, (dict, list)):
        return json.dumps(value, ensure_ascii=False)
    return str(value)


def _variant(value: Any) -> tuple[str, Any]:
    if isinstance(value, dict) and len(value) == 1:
        name, inner = next(iter(value.items()))
        return str(name), inner
    return str(value), None


def status_text(status: Any) -> str:
    name, inner = _variant(status)
    if name == "NotJudged":
        reason, _ = _variant(inner)
        return f"판단 안 함 — {NOT_JUDGED.get(reason, reason)}"
    return STATUS_LABELS.get(name, name)


def _promul(p: Any) -> str:
    if not isinstance(p, dict):
        return str(p or "")
    return f"{p.get('issuer', '')} 제{p.get('number', '')}호"


def version_line(v: dict[str, Any]) -> str:
    unit = v.get("unit") or {}
    period = v.get("valid_period") or {}
    until = period.get("until") or ""
    end = v.get("end_reason") or "현행"
    return (
        f"{unit.get('key', '')} · {_promul(v.get('promul_no'))}({v.get('promul_date', '')} 공포)"
        f" · 시행 {period.get('from', '')}~{until} · 종료 {end}"
    )


def flag_line(flag: Any) -> str:
    name, inner = _variant(flag)
    label = FLAG_LABELS.get(name, name)
    if isinstance(inner, dict):
        detail = inner.get("note") or inner.get("basis") or _compact(inner)
        rule = inner.get("rule_id")
        return f"{label}: {detail}" + (f" [{rule}]" if rule and rule not in str(detail) else "")
    return label


def step_line(step: Any) -> str:
    name, s = _variant(step)
    if not isinstance(s, dict):
        return name
    if name == "RefTime":
        principle = s.get("principle") or {}
        return f"기준시: {principle.get('label', principle.get('id', ''))} → {s.get('time_id', '')}, 날짜 {_compact(s.get('date'))}"
    if name == "RefTimeAnswer":
        return f"기준시({s.get('source', '')}): {s.get('basis', '')}, 날짜 {_compact(s.get('date'))}"
    if name == "Addendum":
        return f"부칙 규칙 {s.get('rule_id', '')} ({s.get('source', '')}) → {s.get('outcome', '')}"
    if name == "AddendumSkipped":
        return f"부칙 규칙 {s.get('rule_id', '')} 건너뜀 — {_promul(s.get('by'))} 전부개정({s.get('effective', '')})"
    if name == "Adjust":
        return f"보정 {s.get('label', '')}: {s.get('note', '')}"
    return f"{name}: {s.get('note', _compact(s))}"


def format_resolution(res: dict[str, Any]) -> list[str]:
    lines = [f"상태: {status_text(res.get('status'))}"]
    if res.get("needs_expert"):
        lines.append(EXPERT)
    versions = res.get("versions") or []
    if versions:
        lines.append("판본:")
        for v in versions:
            lines.append(f"- {version_line(v)}")
            body = str(v.get("body", ""))
            if body:
                lines.append(f"  {body[:BODY_CHARS]}")
    flags = res.get("flags") or []
    if flags:
        lines.append("표시:")
        lines.extend(f"- {flag_line(f)}" for f in flags)
    missing = res.get("missing") or []
    if missing:
        lines.append("모자란 사실:")
        lines.extend(
            f"- {m.get('event', '')}: {MISSING_LABELS.get(m.get('reason', ''), m.get('reason', ''))}" for m in missing
        )
    trace = res.get("trace") or []
    if trace:
        lines.append("근거 체인:")
        lines.extend(f"- {step_line(s)}" for s in trace)
    if versions:
        lines.append(BODY_NOTE)
    return lines


def format_resolve(data: dict[str, Any]) -> str:
    law = data.get("law") or {}
    unit = data.get("unit") or {}
    head = (
        f"[{law.get('name', '')} {unit.get('key', '')} · 법령ID {law.get('law_id', '')}"
        f" · 판단 기준일 {data.get('as_of', '')} · 판본 자료 시작 {data.get('data_from') or '없음'}]"
    )
    return "\n".join([head, *format_resolution(data.get("resolution") or {})])


def cite_status(res: dict[str, Any] | None) -> str:
    if not res:
        return ""
    name, inner = _variant(res.get("status"))
    if name == "Resolved":
        return "해소"
    if name == "MistakeCandidate":
        reason, detail = _variant(inner)
        return f"오기 후보({reason}{' ' + _compact(detail) if detail is not None else ''})"
    if name == "OutsideData":
        first = (inner or {}).get("first") or "없음"
        return f"판본 자료 범위 밖(자료 시작 {first}) — 오기로 보지 않음"
    return name


def cite_line(item: dict[str, Any]) -> str:
    old = "구 " if item.get("old") else ""
    head = f"{old}{item.get('law_name', '')} {item.get('article', '')}".strip() + f" · {item.get('cite_type', '')}"
    if item.get("unresolved"):
        return f"{head} → 해소 안 함: {item['unresolved']}"
    res = item.get("resolution") or {}
    parts = [f"{head} → {cite_status(res)}"]
    if res.get("base_date"):
        parts.append(f"기준일 {res['base_date']}")
    if res.get("version"):
        parts.append(version_line(res["version"]))
    flags = []
    for f in res.get("flags") or []:
        name, inner = _variant(f)
        label = CITE_FLAGS.get(name, name)
        flags.append(f"{label}({_compact(inner)})" if inner is not None else label)
    if flags:
        parts.append("표시 " + ", ".join(flags))
    return " · ".join(parts)


def format_citations(data: dict[str, Any]) -> str:
    items = data.get("items") or []
    lines = [f"[판결문 인용 해소 · 선고일 {data.get('judgment_date', '')}]"]
    lines.extend(f"- {cite_line(i)}" for i in items)
    if not items:
        lines.append("읽은 인용 없음")
    if data.get("truncated"):
        lines.append("한도를 넘어 일부 인용은 해소하지 않음")
    return "\n".join(lines)


def format_trace(data: dict[str, Any]) -> str:
    counts = data.get("counts") or {}
    lines = [
        f"[답변 줄·예측 줄 대조 · 선고일 {data.get('judgment_date', '')}]",
        "대조: " + (", ".join(f"{k} {v}" for k, v in sorted(counts.items())) or "없음"),
    ]
    for i, row in enumerate(data.get("rows") or [], start=1):
        outcome = LANE_LABELS.get(str(row.get("outcome", "")), str(row.get("outcome", "")))
        lines.append(f"{i}. 답변 줄: {cite_line(row.get('answer') or {})}")
        prediction = row.get("prediction")
        if prediction:
            predicted = [version_line(v) for v in prediction.get("versions") or []]
            lines.append(f"   예측 줄: {status_text(prediction.get('status'))} · " + (" / ".join(predicted) or "판본 없음"))
            for f in prediction.get("flags") or []:
                lines.append(f"   - {flag_line(f)}")
        lines.append(f"   결과: {outcome}")
    if data.get("truncated"):
        lines.append("한도를 넘어 일부 인용은 대조하지 않음")
    return "\n".join(lines)


def make_version_tools(client: ToolClient) -> list[BaseTool]:
    @tool(
        "resolve_version",
        description="사건 사실로 법령 조문의 적용 판본을 정한다(기준시 → 판본 조회 → 부칙 → 분야별 보정). "
        "결과는 상태·판본·표시·모자란 사실·근거 체인이다. 「판정 보류」나 「사실이 모자라」면 판본을 단정하지 않는다. "
        f"unit은 「제N조」나 「제N조의M」. {KIND_HELP} {FACT_HELP} as_of는 판단 기준일(없으면 오늘).",
    )
    async def resolve_version(
        law: str,
        unit: str,
        kind: CaseKind,
        facts: list[FactArg],
        state: Literal["Pending", "Finalized"] | None = None,
        as_of: IsoDate | None = None,
    ) -> str:
        body = {"law": law, "unit": unit, "kind": kind, "facts": fact_body(facts), "state": state, "as_of": as_of}
        data, err = await client.fetch("POST", "/versions/resolve", kind="search", fail=SEARCH_FAIL, json=body)
        return err or material(f"판본 결정 {law} {unit}", format_resolve(data))

    @tool(
        "resolve_citations",
        description="판결문(참조조문·이유)의 법령 인용 괄호(「…로 개정되기 전의 것」 등)를 읽어 법원이 적용한 조문 판본으로 해소한다. "
        "judgment_date는 그 판결의 선고일. 「오기 후보」는 판본 자료와 맞지 않는 인용, 「판본 자료 범위 밖」은 자료가 없어 확인하지 못한 인용이다.",
    )
    async def resolve_citations(text: str, judgment_date: IsoDate) -> str:
        body = {"text": text, "judgment_date": judgment_date}
        data, err = await client.fetch("POST", "/versions/citations", kind="search", fail=SEARCH_FAIL, json=body)
        return err or material("판결문 인용 해소", format_citations(data))

    @tool(
        "trace_judgment_versions",
        description="한 판결문에서 답변 줄(인용 괄호를 해소한 판본)과 예측 줄(사실과 선고일만으로 판본 결정 엔진이 고른 판본)을 "
        f"조문마다 대조한다. judgment_date는 선고일. {KIND_HELP} {FACT_HELP}",
    )
    async def trace_judgment_versions(
        text: str,
        judgment_date: IsoDate,
        kind: CaseKind,
        facts: list[FactArg],
        state: Literal["Pending", "Finalized"] | None = None,
    ) -> str:
        body = {
            "text": text,
            "judgment_date": judgment_date,
            "kind": kind,
            "facts": fact_body(facts),
            "state": state,
        }
        data, err = await client.fetch("POST", "/versions/trace", kind="search", fail=SEARCH_FAIL, json=body)
        return err or material("답변 줄·예측 줄 대조", format_trace(data))

    return [resolve_version, resolve_citations, trace_judgment_versions]
