use std::fmt::Write;

use crate::source::RuleFile;
use crate::validate::Report;

pub fn render(file: &RuleFile, report: &Report) -> String {
    let mut out = String::new();
    let e = report.expected;
    let a = report.actual;
    let mark = |x: usize, y: usize| if x == y { "맞음" } else { "다름" };
    let _ = writeln!(out, "청구 유형: {}", report.claim_type);
    if let Some(status) = &file.status {
        let _ = writeln!(out, "상태: {status}");
    }
    let _ = writeln!(
        out,
        "수(실제/기대): 노드 {}/{} {}, 간선 {}/{} {}, 리프 {}/{} {}",
        a.nodes,
        e.nodes,
        mark(a.nodes, e.nodes),
        a.edges,
        e.edges,
        mark(a.edges, e.edges),
        a.leaves,
        e.leaves,
        mark(a.leaves, e.leaves),
    );
    if report.structure_ok() {
        let _ = writeln!(out, "구조 검사: 통과");
    } else {
        let _ = writeln!(out, "구조 검사: 실패");
        for issue in &report.structure {
            let _ = writeln!(out, "  - {issue}");
        }
        for issue in &report.other {
            let _ = writeln!(out, "  - {issue}");
        }
    }
    if report.banned.is_empty() {
        let _ = writeln!(out, "결론성 평가어: 없음");
    } else {
        let _ = writeln!(out, "결론성 평가어: {}건", report.banned.len());
        for (id, word) in &report.banned {
            let _ = writeln!(out, "  - {id}: 「{word}」");
        }
    }
    if !report.burden_missing.is_empty() {
        let ids: Vec<String> = report.burden_missing.iter().map(ToString::to_string).collect();
        let _ = writeln!(out, "입증책임: 값이 없는 노드 {}개: {}", ids.len(), ids.join(", "));
    } else if report.burden_pending.is_empty() {
        let _ = writeln!(out, "입증책임: 정해짐");
    } else {
        let _ = writeln!(out, "입증책임: 미정(노드 {}개)", report.burden_pending.len());
    }
    if !file.excluded_from_tree.is_empty() {
        let _ = writeln!(out, "트리에서 뺀 항목: {}개", file.excluded_from_tree.len());
        for item in &file.excluded_from_tree {
            let _ = writeln!(out, "  - {}: {}", item.item, item.reason);
        }
    }
    let mut reasons = Vec::new();
    if !report.structure_ok() {
        reasons.push("구조 검사 실패");
    }
    if !report.banned.is_empty() {
        reasons.push("결론성 평가어");
    }
    if !report.burden_missing.is_empty() {
        reasons.push("입증책임 값 없음");
    }
    if !report.burden_pending.is_empty() {
        reasons.push("입증책임 미정");
    }
    if reasons.is_empty() {
        let _ = writeln!(out, "결과: build 가능");
    } else {
        let _ = writeln!(out, "결과: build 거부 — {}", reasons.join(", "));
    }
    out
}
