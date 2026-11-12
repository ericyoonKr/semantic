use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use retrieval::{CaseQuery, CitationItem, CitationsBy, Corpus, CorpusDb, NoEmbedder, RetrievalError};

type Checked = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const URL_VAR: &str = "CORPUS_PG_URL";

fn corpus() -> Result<Option<Corpus>, RetrievalError> {
    let Ok(url) = std::env::var(URL_VAR) else {
        eprintln!("{URL_VAR} 없음 — 실제 판례 DB 시험을 건너뜀");
        return Ok(None);
    };
    let pool = CorpusDb::parse(&url)?.connect_lazy(4, Duration::from_secs(30));
    Ok(Some(Corpus::new(pool, Arc::new(NoEmbedder))))
}

fn check(ok: bool, what: impl std::fmt::Display) -> Checked {
    if ok { Ok(()) } else { Err(what.to_string().into()) }
}

#[tokio::test]
async fn named_article_brings_citing_cases_and_majority_only() -> Checked {
    let Some(c) = corpus()? else { return Ok(()) };
    let q = CaseQuery { text: "민법 제750조 불법행위 손해배상 책임".into(), k: 10, ..CaseQuery::default() };
    let started = Instant::now();
    let r = retrieval::search_cases(&c, &q).await?;
    eprintln!("판례 검색 {:?}, 결과 {}건", started.elapsed(), r.hits.len());
    for h in &r.hits {
        eprintln!("  {} {} {} {} {:.4}", h.case_no, h.section_role, h.matched, h.opinion.as_deref().unwrap_or("-"), h.score);
    }
    check(r.degraded_reason.as_deref() == Some(retrieval::search::VECTOR_EMPTY), format!("{:?}", r.degraded_reason))?;
    check(r.hits.len() == 10, format!("{} hits", r.hits.len()))?;
    check(r.hits.iter().any(|h| h.matched.contains("cited")), "cited branch reached the top 10")?;
    check(r.hits.iter().all(|h| matches!(h.opinion.as_deref(), None | Some("majority"))), "majority only")?;
    Ok(())
}

#[tokio::test]
async fn read_case_hides_minority_until_asked_and_names_the_overruling_case() -> Checked {
    let Some(c) = corpus()? else { return Ok(()) };
    let majority = retrieval::read_case(c.pool(), 4554, None, false).await?;
    let all = retrieval::read_case(c.pool(), 4554, None, true).await?;
    let minority = |d: &retrieval::CaseDoc| {
        d.sections.iter().filter(|s| matches!(s.opinion.as_deref(), Some("dissent" | "separate" | "concurring"))).count()
    };
    eprintln!("2021마6542 절 {} / 소수의견 포함 {}", majority.sections.len(), all.sections.len());
    check(majority.en_banc && minority(&majority) == 0 && minority(&all) == 7, format!("{} {}", minority(&majority), minority(&all)))?;
    let old = retrieval::read_case(c.pool(), 37, None, false).await?;
    check(old.case_no == "2019두50946" && old.overruled_by.contains(&"2021두59908".to_string()), format!("{:?}", old.overruled_by))?;
    Ok(())
}

#[tokio::test]
async fn citing_cases_follow_the_article_key() -> Checked {
    let Some(c) = corpus()? else { return Ok(()) };
    let by = CitationsBy::LawUnit { law: "민법".into(), unit: "제750조".into() };
    let r = retrieval::citations(c.pool(), &by, 1000).await?;
    let ids: HashSet<i64> = r
        .items
        .iter()
        .filter_map(|i| match i {
            CitationItem::Citing(c) => Some(c.decision_id),
            CitationItem::Cited(_) => None,
        })
        .collect();
    eprintln!("민법 제750조 인용 행 {}, 판례 {}", r.items.len(), ids.len());
    check(ids.len() >= 181, format!("{} decisions", ids.len()))?;
    Ok(())
}
