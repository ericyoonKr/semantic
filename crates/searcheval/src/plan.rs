use crate::call::{Call, Target};
use crate::sample::{GoldLaw, Item};
use crate::score::{Gold, Scoring, norm};

const MAX_K: usize = 80;

pub struct Job {
    pub call: Call,
    pub exclude: Vec<String>,
    pub scorings: Vec<Scoring>,
}

pub struct Plan {
    pub jobs: Vec<Job>,
    pub skipped: Vec<(String, String)>,
}

fn call(item: &Item, target: Target, k: usize, extra: usize) -> Call {
    let filters = item.filters.clone().unwrap_or_default();
    Call {
        id: item.id.clone(),
        target,
        query: item.query.clone().unwrap_or_default(),
        as_of: filters.as_of,
        decided_to: filters.decided_to,
        params: item.params.clone(),
        k: (k + extra).min(MAX_K),
    }
}

fn unnamed(gold: &[GoldLaw]) -> Vec<GoldLaw> {
    gold.iter().filter(|g| !g.named_in_query).cloned().collect()
}

pub fn build(items: &[Item], k: usize, cases_from: Option<&str>) -> Plan {
    let mut jobs = Vec::new();
    let mut skipped = Vec::new();
    for item in items {
        let exclude: Vec<String> = item.exclude_cases.iter().map(|c| norm(&c.case_no)).collect();
        let cases: Vec<_> = item
            .gold_cases
            .iter()
            .filter(|g| match (cases_from, &g.decided_on) {
                (Some(from), Some(d)) => d.as_str() >= from,
                (Some(_), None) => false,
                (None, _) => true,
            })
            .cloned()
            .collect();
        let dropped_cases = cases.len() < item.gold_cases.len();
        match item.kind.as_str() {
            "mcq" => {}
            "retrieval" => {
                if item.query.as_deref().is_none_or(str::is_empty) {
                    skipped.push((item.id.clone(), "질의가 비어 있음".to_string()));
                    continue;
                }
                if !item.gold_laws.is_empty() {
                    let mut scorings = vec![Scoring {
                        group: "판례 검색 표본 · 법령 (정답 전부)".to_string(),
                        gold: Gold::Laws(item.gold_laws.clone()),
                    }];
                    let rest = unnamed(&item.gold_laws);
                    if !rest.is_empty() {
                        scorings.push(Scoring {
                            group: "판례 검색 표본 · 법령 (질의 글에 이름과 조문이 나온 정답은 뺌)"
                                .to_string(),
                            gold: Gold::Laws(rest),
                        });
                    }
                    jobs.push(Job {
                        call: call(item, Target::Laws, k, 0),
                        exclude: exclude.clone(),
                        scorings,
                    });
                }
                if !cases.is_empty() {
                    jobs.push(Job {
                        call: call(item, Target::Cases, k, exclude.len()),
                        exclude,
                        scorings: vec![Scoring {
                            group: "판례 검색 표본 · 판례 (질의를 낸 판례는 후보에서 뺌)".to_string(),
                            gold: Gold::Cases(cases),
                        }],
                    });
                } else if dropped_cases {
                    skipped.push((item.id.clone(), "정답 판례가 모두 --cases-from 앞".to_string()));
                }
            }
            "lookup" => {
                let Some(target) = item.target.as_deref().and_then(Target::parse) else {
                    skipped.push((item.id.clone(), "target이 laws·cases·unit이 아님".to_string()));
                    continue;
                };
                let gold = match target {
                    Target::Cases => Gold::Cases(cases),
                    _ => Gold::Laws(item.gold_laws.clone()),
                };
                let empty = match &gold {
                    Gold::Cases(g) => g.is_empty(),
                    Gold::Laws(g) => g.is_empty(),
                };
                if empty || (target == Target::Unit && item.params.is_none()) {
                    skipped.push((item.id.clone(), "정답 또는 params 없음".to_string()));
                    continue;
                }
                let label = match target {
                    Target::Cases => "바로 찾기 표본 · 사건번호 → 판례 검색",
                    Target::Laws => "바로 찾기 표본 · 법령 약칭과 조문 → 법령 검색",
                    Target::Unit => "바로 찾기 표본 · 법령명과 조문 → 조문 읽기",
                };
                jobs.push(Job {
                    call: call(item, target, k, exclude.len()),
                    exclude,
                    scorings: vec![Scoring {
                        group: label.to_string(),
                        gold,
                    }],
                });
            }
            other => skipped.push((item.id.clone(), format!("모르는 kind: {other}"))),
        }
    }
    Plan { jobs, skipped }
}
