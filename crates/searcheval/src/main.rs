mod call;
mod mcq;
mod plan;
mod report;
mod sample;
mod score;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use futures::StreamExt;

use crate::call::{Http, Outcome};
use crate::sample::Item;
use crate::score::Tally;

const SCHEMA: &str = "표본 파일(JSONL, 한 줄에 한 항목). 모든 줄: id, kind(mcq | retrieval | lookup).
  retrieval: query, gold_laws[{law, unit, named_in_query}], gold_cases[{court, case_no, decided_on}],
             exclude_cases[{court, case_no}](후보에서 뺄 판례), filters{as_of, decided_to}
             → 법령 정답이 있으면 POST /search/laws, 판례 정답이 있으면 POST /search/cases
  lookup:    target(cases | laws | unit), query, gold_cases 또는 gold_laws, unit이면 params{law, unit}
             → cases·laws는 검색, unit은 GET /laws/unit
  mcq:       question, choices, gold{official_answers} → --predictions의 답과 맞춰 정답률만 냄
  그 밖의 칸(출처 등)은 읽지 않는다.
맞춤: 법령은 법령명(띄어쓰기·가운뎃점 무시)과 조 번호(제N조, 제N조의M, 부칙 제N조)가 같으면,
      판례는 사건번호(띄어쓰기 무시)가 같으면 정답이다.
기록 파일(--recorded·--save, JSONL): {id, target, status(ok | not_found | failed), reason?, latency_ms, hits[]}
      hits는 서버 응답의 hits 모양 그대로(unit은 응답 객체 하나).
검색 실패: 연결 안 됨·시간 초과·5xx·403 등. 실패한 호출은 정답 없음으로 세고 사유별로 따로 센다.";

#[derive(Debug, Parser)]
#[command(name = "searcheval", about = "법령·판례 검색 평가 실행기", long_about = None, after_long_help = SCHEMA)]
struct Args {
    #[arg(help = "표본 파일 경로(JSONL)")]
    sample: PathBuf,
    #[arg(long, env = "SEARCHEVAL_BASE_URL", default_value = "http://127.0.0.1:8080", help = "서버 ①의 주소")]
    base_url: String,
    #[arg(long, env = "INTERNAL_KEY", hide_env_values = true, help = "서버 ①의 내부 키(x-internal-key)")]
    internal_key: Option<String>,
    #[arg(long, help = "HTTP 대신 이 기록 파일의 결과로 잰다(이전 방식의 실측값용)")]
    recorded: Option<PathBuf>,
    #[arg(long, help = "호출 결과를 기록 파일 형식으로 저장")]
    save: Option<PathBuf>,
    #[arg(long, default_value_t = 20, help = "순위를 볼 결과 수(1~80)")]
    k: usize,
    #[arg(long, default_value_t = 10_000, help = "호출 하나의 시간 상한(ms)")]
    timeout_ms: u64,
    #[arg(long, default_value_t = 1, help = "동시 호출 수. 기본 1은 지연에 대기 시간이 섞이지 않게 하려는 값")]
    concurrency: usize,
    #[arg(long, help = "이 날짜(YYYY-MM-DD) 앞에 선고된 정답 판례는 뺀다(DB 기간 밖 정답 제외)")]
    cases_from: Option<String>,
    #[arg(long, value_delimiter = ',', default_value = "claims,opinion_dissent,opinion_separate,lower_court", help = "법원 판단이 아닌 글로 셀 절 역할")]
    non_court_roles: Vec<String>,
    #[arg(long, help = "선택형 답 기록(JSONL: {id, answer})")]
    predictions: Option<PathBuf>,
}

async fn outcomes(args: &Args, calls: Vec<call::Call>) -> Result<Vec<Outcome>> {
    if let Some(path) = &args.recorded {
        let map = call::load_recorded(path)?;
        return Ok(calls.iter().map(|c| call::from_recorded(&map, c)).collect());
    }
    let key = args.internal_key.clone().unwrap_or_default();
    if key.is_empty() {
        println!("알림: INTERNAL_KEY가 없어 서버가 거부할 수 있습니다");
    }
    let http = Http::new(&args.base_url, &key, Duration::from_millis(args.timeout_ms))?;
    let http = &http;
    Ok(futures::stream::iter(calls)
        .map(|c| async move { http.run(&c).await })
        .buffered(args.concurrency.max(1))
        .collect()
        .await)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if !(1..=80).contains(&args.k) {
        anyhow::bail!("--k는 1부터 80까지입니다");
    }
    let items: Vec<Item> = sample::read_jsonl(&args.sample)?;
    let plan = plan::build(&items, args.k, args.cases_from.as_deref());
    let source = match &args.recorded {
        Some(p) => format!("기록 파일 {}", p.display()),
        None => format!("서버 {}", args.base_url),
    };
    println!(
        "표본 {} · 항목 {}개 · 호출 {}건 · {} · k={}",
        args.sample.display(),
        items.len(),
        plan.jobs.len(),
        source,
        args.k
    );
    if !plan.skipped.is_empty() {
        println!("건너뛴 호출 {}건", plan.skipped.len());
        let mut why = std::collections::BTreeMap::<&str, usize>::new();
        for (_, r) in &plan.skipped {
            *why.entry(r.as_str()).or_default() += 1;
        }
        for (r, c) in why {
            println!("  {r}: {c}건");
        }
    }
    let calls = plan.jobs.iter().map(|j| j.call.clone()).collect();
    let results = outcomes(&args, calls).await?;
    if let Some(path) = &args.save {
        call::save(path, &results)?;
    }
    let mut order: Vec<String> = Vec::new();
    let mut tallies = std::collections::HashMap::<String, Tally>::new();
    let mut cases_all = Tally::default();
    for (job, o) in plan.jobs.iter().zip(results.iter()) {
        let hits = score::visible(o, &job.exclude, args.k);
        for s in &job.scorings {
            if !tallies.contains_key(&s.group) {
                order.push(s.group.clone());
            }
            let t = tallies.entry(s.group.clone()).or_default();
            score::add(t, o, &hits, &s.gold, &args.non_court_roles);
        }
        if o.target == call::Target::Cases
            && let Some(s) = job.scorings.first()
        {
            score::add(&mut cases_all, o, &hits, &s.gold, &args.non_court_roles);
        }
    }
    for g in &order {
        if let Some(t) = tallies.get(g) {
            report::group(g, t);
        }
    }
    if cases_all.n > 0 {
        report::non_court(&cases_all, &args.non_court_roles);
    }
    mcq::report(&items, args.predictions.as_deref())?;
    Ok(())
}
