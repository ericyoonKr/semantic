mod common;
mod fixture;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{FixedEmbedder, corpus, day, with_fixture};
use fixture::{Checked, Fixture, check};
use retrieval::{CaseHit, CaseQuery, CaseWeights, NoEmbedder};

fn query(text: &str) -> CaseQuery {
    CaseQuery { text: text.into(), k: 10, ..CaseQuery::default() }
}

fn hit<'a>(hits: &'a [CaseHit], section_role: &str, decision_id: i64) -> Result<&'a CaseHit, fixture::Failure> {
    hits.iter()
        .find(|h| h.section_role == section_role && h.decision_id == decision_id)
        .ok_or_else(|| format!("{section_role} of {decision_id} expected in {hits:?}").into())
}

async fn vector_failure_checks(fx: &Fixture) -> Checked {
    let embedder = Arc::new(FixedEmbedder { calls: AtomicUsize::new(0) });
    let c = corpus(fx, embedder.clone())?;
    let cases = retrieval::search_cases(&c, &CaseQuery { k: 5, ..query("공무원 과실") }).await?;
    check(cases.degraded, "case vector failure must degrade")?;
    let reason = cases.degraded_reason.as_deref();
    check(
        reason == Some(retrieval::search::VECTOR_FAILED) || reason == Some(retrieval::search::VECTOR_EMPTY),
        format!("{reason:?}"),
    )?;
    check(!cases.hits.is_empty(), "case text hits still returned")?;
    check(embedder.calls.load(Ordering::SeqCst) == 0, "no embedding call when vector tables are absent")?;
    Ok(())
}

#[tokio::test]
async fn case_vector_path_with_fake_embedder_degrades_when_vector_tables_are_absent() -> Checked {
    with_fixture(|fx| async move {
        let r = vector_failure_checks(&fx).await;
        (fx, r)
    })
    .await
}

async fn case_search_checks(fx: &Fixture) -> Checked {
    let c = corpus(fx, Arc::new(NoEmbedder))?;
    let all = retrieval::search_cases(&c, &query("공무원 과실")).await?;
    check(all.degraded, "degraded without embedder")?;
    check(all.hits.iter().all(|h| h.decision_id != 12), "bundle rows are excluded")?;
    check(all.hits.iter().all(|h| h.opinion.as_deref() != Some("dissent")), "minority sections are left out by default")?;
    let reasons = hit(&all.hits, "reasons", 10)?;
    check(reasons.text == fixture::REASONS_HEAD, format!("hit text should be the matching chunk, got {:?}", reasons.text))?;
    check(reasons.lower_court_view, "chunk inside the lower court view is marked")?;
    check(reasons.en_banc && reasons.disposition == "reverse_remand", "decision marks")?;
    let issue = hit(&all.hits, "holding_issue", 10)?;
    check(issue.direction.as_deref() == Some("negative") && !issue.lower_court_view, "holding issue direction")?;
    let summary = hit(&all.hits, "holding_summary", 11)?;
    check(summary.text == fixture::SUMMARY_11, "section without chunks returns its body")?;
    check(summary.court == "서울고등법원" && summary.case_no == "2022누2222", "court and case number")?;
    check(summary.field == "administrative" && summary.overruled_by.is_empty(), "inactive overrule run is not read")?;
    check(summary.matched == "text", format!("matched {}", summary.matched))?;
    let old = hit(&all.hits, "holding_summary", 13)?;
    check(old.overruled_by == ["2020다1111"], format!("overruled by {:?}", old.overruled_by))?;
    let json = serde_json::to_value(&all)?;
    let got: Vec<String> = json["hits"][0].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
    let want = [
        "case_no", "court", "decided_on", "decision_id", "disposition", "en_banc", "field", "lower_court_view", "matched",
        "score", "section_role", "text",
    ];
    check(want.iter().all(|w| got.iter().any(|g| g == w)), format!("case hit fields {got:?}"))?;

    let minority = retrieval::search_cases(&c, &CaseQuery { include_minority: true, ..query("공무원 과실") }).await?;
    check(minority.hits.iter().any(|h| h.opinion.as_deref() == Some("dissent")), "minority sections when asked")?;

    let supreme = retrieval::search_cases(&c, &CaseQuery { court_level: Some(vec!["supreme".into()]), ..query("공무원 과실") }).await?;
    check(!supreme.hits.is_empty() && supreme.hits.iter().all(|h| h.court == "대법원"), "court level filter")?;

    let dated = retrieval::search_cases(
        &c,
        &CaseQuery {
            decided_from: day(2023, 1, 1),
            decided_to: day(2023, 12, 31),
            field: Some(vec!["administrative".into()]),
            ..query("공무원 과실")
        },
    )
    .await?;
    check(dated.hits.len() == 1 && dated.hits[0].decision_id == 11, format!("date and field filters, got {:?}", dated.hits))?;
    Ok(())
}

#[tokio::test]
async fn case_search_keeps_majority_marks_sections_and_returns_matching_chunk() -> Checked {
    with_fixture(|fx| async move {
        let r = case_search_checks(&fx).await;
        (fx, r)
    })
    .await
}

async fn case_rank_checks(fx: &Fixture) -> Checked {
    let flat = corpus(fx, Arc::new(NoEmbedder))?.with_case_weights(CaseWeights { holding: 1.0, overruled: 1.0 });
    let base = retrieval::search_cases(&flat, &query("공무원 과실")).await?;
    let sink = corpus(fx, Arc::new(NoEmbedder))?.with_case_weights(CaseWeights { holding: 1.0, overruled: 0.1 });
    let sunk = retrieval::search_cases(&sink, &query("공무원 과실")).await?;
    check(base.hits.len() == sunk.hits.len(), "same hits")?;
    check(sunk.hits.last().map(|h| h.decision_id) == Some(13), format!("overruled case sinks {:?}", sunk.hits))?;
    let lift = corpus(fx, Arc::new(NoEmbedder))?.with_case_weights(CaseWeights { holding: 5.0, overruled: 1.0 });
    let lifted = retrieval::search_cases(&lift, &query("공무원 과실")).await?;
    let first_reasons = lifted.hits.iter().position(|h| h.section_role == "reasons").ok_or("reasons expected")?;
    check(
        lifted.hits[..first_reasons].iter().all(|h| h.section_role.starts_with("holding_")) && first_reasons >= 4,
        format!("holding sections first {:?}", lifted.hits.iter().map(|h| &h.section_role).collect::<Vec<_>>()),
    )?;
    Ok(())
}

#[tokio::test]
async fn holding_weight_lifts_and_overrule_weight_sinks() -> Checked {
    with_fixture(|fx| async move {
        let r = case_rank_checks(&fx).await;
        (fx, r)
    })
    .await
}

async fn cited_branch_checks(fx: &Fixture) -> Checked {
    let c = corpus(fx, Arc::new(NoEmbedder))?;
    let named = retrieval::search_cases(&c, &query("국가배상법 제2조 공무원 과실")).await?;
    let from_11 = hit(&named.hits, "holding_summary", 11)?;
    check(from_11.matched == "text+cited", format!("article in the query {}", from_11.matched))?;
    let none = retrieval::search_cases(&c, &query("국가배상법 제3조 공무원 과실")).await?;
    check(none.hits.iter().all(|h| !h.matched.contains("cited")), "only the inactive run cited 제3조")?;

    let merged = retrieval::search_cases(
        &c,
        &CaseQuery { article_keys: Some(vec!["L1:0002".into()]), ..query("소속 부정") },
    )
    .await?;
    check(hit(&merged.hits, "reasons", 10)?.matched == "text+cited", format!("cited joins the text section {:?}", merged.hits))?;
    check(
        merged.hits.iter().all(|h| !(h.decision_id == 10 && h.section_role == "holding_summary")),
        "no second section for the same cited decision",
    )?;
    let keyed = retrieval::search_cases(
        &c,
        &CaseQuery { article_keys: Some(vec!["L2:0003".into()]), ..query("전혀없는낱말") },
    )
    .await?;
    check(keyed.hits.len() == 1, format!("cited branch alone {:?}", keyed.hits))?;
    check(
        keyed.hits[0].decision_id == 10 && keyed.hits[0].section_role == "holding_summary" && keyed.hits[0].matched == "cited",
        format!("representative section {:?}", keyed.hits[0]),
    )?;
    Ok(())
}

#[tokio::test]
async fn cases_citing_the_named_article_join_the_fusion() -> Checked {
    with_fixture(|fx| async move {
        let r = cited_branch_checks(&fx).await;
        (fx, r)
    })
    .await
}

#[tokio::test]
async fn vector_path_finds_what_text_search_cannot_when_pgvector_is_present() -> Checked {
    let Some(fx) = fixture::open_vectors().await? else {
        return Ok(());
    };
    let result = async {
        let embedder = Arc::new(FixedEmbedder {
            calls: AtomicUsize::new(0),
        });
        let c = corpus(&fx, embedder.clone())?;
        let cases = retrieval::search_cases(
            &c,
            &CaseQuery { k: 5, ..query("보상 규모") },
        )
        .await?;
        check(!cases.degraded, "case vector path must not degrade")?;
        check(
            cases.hits.first().map(|h| h.text.as_str()) == Some(fixture::REASONS_TAIL),
            format!("nearest chunk text chosen: {:?}", cases.hits.first().map(|h| &h.text)),
        )?;
        check(embedder.calls.load(Ordering::SeqCst) == 1, "embedder called once per search")?;
        Ok::<(), fixture::Failure>(())
    }
    .await;
    fx.close().await?;
    result
}
