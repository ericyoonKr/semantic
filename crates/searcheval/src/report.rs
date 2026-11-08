use crate::score::Tally;

fn pct(a: usize, n: usize) -> String {
    if n == 0 {
        return "-".to_string();
    }
    format!("{:.1}% ({a}/{n})", 100.0 * a as f64 / n as f64)
}

fn quantile(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let pos = (q * (sorted.len() - 1) as f64).round() as usize;
    sorted.get(pos.min(sorted.len() - 1)).copied()
}

fn bucket(r: Option<usize>) -> usize {
    match r {
        Some(1) => 0,
        Some(2) => 1,
        Some(3) => 2,
        Some(4) => 3,
        Some(5) => 4,
        Some(6..=10) => 5,
        Some(11..=20) => 6,
        Some(_) => 7,
        None => 8,
    }
}

const BUCKETS: [&str; 9] = ["1", "2", "3", "4", "5", "6~10", "11~20", "21~", "없음"];

pub fn group(name: &str, t: &Tally) {
    println!("== {name}");
    println!(
        "  호출 {}건: 응답 {} · 찾지 못함 응답 {} · 검색 실패 {}",
        t.n, t.ok, t.not_found, t.failed
    );
    for (reason, c) in &t.reasons {
        println!("    {reason}: {c}건");
    }
    let within = |k: usize| t.ranks.iter().filter(|r| r.is_some_and(|x| x <= k)).count();
    println!("  상위 5 안에 정답: {}", pct(within(5), t.n));
    println!("  상위 10 안에 정답: {}", pct(within(10), t.n));
    let mrr: f64 = t
        .ranks
        .iter()
        .map(|r| r.map_or(0.0, |x| 1.0 / x as f64))
        .sum::<f64>();
    if t.n > 0 {
        println!("  MRR: {:.4}", mrr / t.n as f64);
    }
    let mut counts = [0usize; 9];
    for r in &t.ranks {
        if let Some(c) = counts.get_mut(bucket(*r)) {
            *c += 1;
        }
    }
    let dist: Vec<String> = BUCKETS
        .iter()
        .zip(counts.iter())
        .map(|(b, c)| format!("{b}:{c}"))
        .collect();
    println!("  첫 정답 순위 분포: {}", dist.join(" "));
    let mut lat = t.latencies.clone();
    lat.sort_by(f64::total_cmp);
    match (quantile(&lat, 0.5), quantile(&lat, 0.95)) {
        (Some(p50), Some(p95)) => println!(
            "  지연(검색 실패 뺀 {}건): 중간값 {p50:.1}ms · 95% 지점 {p95:.1}ms",
            lat.len()
        ),
        _ => println!("  지연: 응답이 온 호출이 없어 잴 수 없음"),
    }
}

pub fn non_court(t: &Tally, roles: &[String]) {
    println!("== 법원 판단이 아닌 글을 가져온 수 (판례 결과 {}건 중)", t.hits);
    println!(
        "  문단 표지(speaker·opinion·accepted) 기준: {}건 — 표지 칸이 없는 결과 {}건은 0으로 셈",
        t.non_court_label, t.label_missing
    );
    println!(
        "  절 역할(section_role이 {} 중 하나) 기준: {}건",
        roles.join("·"),
        t.non_court_role
    );
}
