use std::fmt::Write;

use chrono::{Datelike, NaiveDate, Weekday};
use engine::{
    ClaimGraph, DateSource, DateUse, HolidayNote, PeriodCase, PeriodLength, PeriodReport,
    StartRule, Verdict,
};

use crate::verdict_text::label_of;

fn spoken(name: &str) -> String {
    name.replace('_', " ")
}

fn day(d: NaiveDate) -> String {
    let w = match d.weekday() {
        Weekday::Mon => "월",
        Weekday::Tue => "화",
        Weekday::Wed => "수",
        Weekday::Thu => "목",
        Weekday::Fri => "금",
        Weekday::Sat => "토",
        Weekday::Sun => "일",
    };
    format!("{}({w})", d.format("%Y-%m-%d"))
}

fn closed_kind(d: NaiveDate) -> &'static str {
    match d.weekday() {
        Weekday::Sat => "토요일",
        Weekday::Sun => "일요일",
        _ => "공휴일",
    }
}

fn length(p: &PeriodReport) -> String {
    match p.length {
        PeriodLength::Days(n) => format!("{}부터 {n}일", spoken(&p.from.name)),
        PeriodLength::Years(n) => format!("{}부터 {n}년(역에 따라 계산, 민법 제160조)", spoken(&p.from.name)),
    }
}

fn where_from(source: &Option<DateSource>) -> String {
    match source {
        Some(DateSource::Document { page: Some(n) }) => format!("업로드 문서 {n}쪽"),
        Some(DateSource::Document { page: None }) => "업로드 문서".to_string(),
        Some(DateSource::User { fact_id }) => format!("사용자 진술 #{fact_id}에 근거함"),
        Some(DateSource::Today) | None => String::new(),
    }
}

fn date_line(out: &mut String, role: &str, d: &DateUse) {
    let name = spoken(&d.name);
    match (d.date, &d.source) {
        (Some(date), Some(DateSource::Today)) => {
            let _ = writeln!(
                out,
                "- {role}({name}): 사실 자료에 없어 판정일 {}(한국 시각)을 {name}로 보고 계산함",
                day(date)
            );
        }
        (Some(date), source) => {
            let quote = d.quote.as_deref().unwrap_or_default();
            let _ = writeln!(out, "- {role}({name}): {} — 「{quote}」 ({})", day(date), where_from(source));
        }
        (None, _) => {
            let _ = writeln!(out, "- {role}({name}): 확인하지 못함");
        }
    }
}

fn start_line(out: &mut String, p: &PeriodReport) {
    let quote = p.start_quote.as_deref().unwrap_or_default();
    let _ = match p.start {
        StartRule::FirstDayExcluded => writeln!(out, "- 초일: 넣지 않음(민법 제157조 본문) — 사건 시각 「{quote}」"),
        StartRule::FirstDayIncluded => writeln!(out, "- 초일: 넣음(오전 0시부터 시작, 민법 제157조 단서) — 「{quote}」"),
        StartRule::BothChecked => writeln!(
            out,
            "- 초일: 사건이 오전 0시에 시작했는지 사실 자료에 없어 넣지 않는 경우와 넣는 경우를 모두 계산함(민법 제157조)"
        ),
    };
}

fn case_line(out: &mut String, c: &PeriodCase, both: bool) {
    let head = match (both, c.first_day_included) {
        (false, _) => "- 말일",
        (true, false) => "- 말일(초일을 넣지 않으면)",
        (true, true) => "- 말일(초일을 넣으면)",
    };
    if c.skipped.is_empty() {
        let _ = writeln!(out, "{head}: {}", day(c.last));
        return;
    }
    let moved = c
        .skipped
        .iter()
        .map(|d| format!("{} {}", day(*d), closed_kind(*d)))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "{head}: {} — {moved}이라 {}로 미룸(민법 제161조)",
        day(c.end),
        day(c.last)
    );
}

fn holiday_line(out: &mut String, p: &PeriodReport) {
    let mut missing: Vec<i32> = p.cases.iter().flat_map(|c| c.uncovered_years.iter().copied()).collect();
    missing.sort_unstable();
    missing.dedup();
    let lunar = "설날·추석 등 음력 공휴일과 대체공휴일·임시공휴일은 적용하지 않음";
    let _ = match &p.holidays {
        HolidayNote::NotSet => writeln!(out, "- 공휴일 자료: 공휴일 파일이 설정되지 않아 토요일·일요일과 양력 고정 공휴일만 보았고, {lunar}"),
        HolidayNote::Failed(why) => writeln!(out, "- 공휴일 자료: 공휴일 파일을 읽지 못해({why}) 토요일·일요일과 양력 고정 공휴일만 보았고, {lunar}"),
        HolidayNote::Applied if missing.is_empty() => writeln!(out, "- 공휴일 자료: 공휴일 파일의 그 해 목록을 적용함"),
        HolidayNote::Applied => {
            let years = missing.iter().map(|y| format!("{y}년")).collect::<Vec<_>>().join("·");
            writeln!(out, "- 공휴일 자료: 공휴일 파일에 {years} 목록이 없어 그 해는 토요일·일요일과 양력 고정 공휴일만 보았고, {lunar}")
        }
    };
}

fn result_line(out: &mut String, p: &PeriodReport) {
    let to = p.to.date.map(day).unwrap_or_default();
    let _ = match (p.value, p.lo) {
        (Some(true), Some(lo)) => writeln!(out, "- 결과: {to}이 말일 {} 이내임 — 참", day(lo)),
        (Some(false), _) => writeln!(out, "- 결과: {to}이 말일 {}을 지남 — 거짓", p.hi.map(day).unwrap_or_default()),
        _ => writeln!(out, "- 결과: 판정 불가"),
    };
}

pub fn periods(out: &mut String, v: &Verdict, tree: &ClaimGraph) {
    for p in &v.periods {
        let label = label_of(tree, &p.node_id);
        let _ = writeln!(out, "기간 계산 ({} {label}): {}", p.node_id, length(p));
        date_line(out, "기산일", &p.from);
        date_line(out, "비교할 날", &p.to);
        if p.cases.is_empty() {
            result_line(out, p);
            continue;
        }
        start_line(out, p);
        let both = p.cases.len() > 1;
        for c in &p.cases {
            case_line(out, c, both);
        }
        holiday_line(out, p);
        result_line(out, p);
    }
}
