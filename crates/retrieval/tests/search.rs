mod common;
mod fixture;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{FixedEmbedder, corpus, day, with_fixture};
use fixture::{Checked, Fixture, check};
use retrieval::{CaseQuery, NoEmbedder};

async fn vector_failure_checks(fx: &Fixture) -> Checked {
    let embedder = Arc::new(FixedEmbedder {
        calls: AtomicUsize::new(0),
    });
    let c = corpus(fx, embedder.clone())?;
    let cases = retrieval::search_cases(
        &c,
        &CaseQuery {
            text: "공무원 과실".into(),
            k: 5,
            ..CaseQuery::default()
        },
    )
    .await?;
    check(cases.degraded, "case vector failure must degrade")?;
    check(!cases.hits.is_empty(), "case text hits still returned")?;
    check(
        embedder.calls.load(Ordering::SeqCst) == 1,
        "embedder called once per search",
    )?;
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
    let all = retrieval::search_cases(
        &c,
        &CaseQuery {
            text: "공무원 과실".into(),
            k: 10,
            ..CaseQuery::default()
        },
    )
    .await?;
    check(all.degraded, "degraded without embedder")?;
    check(
        all.hits.iter().all(|h| h.decision_id != 12),
        "bundle rows are excluded",
    )?;
    check(all.hits.len() == 3, format!("three sections expected, got {:?}", all.hits))?;
    let reasons = all
        .hits
        .iter()
        .find(|h| h.section_role == "reasons")
        .ok_or("reasons section expected")?;
    check(
        reasons.text == fixture::REASONS_TAIL,
        format!("hit text should be the matching chunk, got {:?}", reasons.text),
    )?;
    let summary = all
        .hits
        .iter()
        .find(|h| h.decision_id == 11)
        .ok_or("decision 11 expected")?;
    check(summary.text == fixture::SUMMARY_11, "section without chunks returns its body")?;
    check(summary.court == "서울고등법원", "court name")?;
    check(summary.case_no == "2022누2222", "case number")?;
    check(summary.field == "administrative", "field")?;
    let json = serde_json::to_value(&all)?;
    let mut got: Vec<String> = json["hits"][0]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    got.sort_unstable();
    let want = [
        "case_no", "court", "decided_on", "decision_id", "field", "score", "section_role", "text",
    ];
    check(got == want, format!("case hit fields {got:?}"))?;

    let supreme = retrieval::search_cases(
        &c,
        &CaseQuery {
            text: "공무원 과실".into(),
            k: 10,
            court_level: Some(vec!["supreme".into()]),
            ..CaseQuery::default()
        },
    )
    .await?;
    check(
        !supreme.hits.is_empty() && supreme.hits.iter().all(|h| h.court == "대법원"),
        "court level filter",
    )?;

    let dated = retrieval::search_cases(
        &c,
        &CaseQuery {
            text: "공무원 과실".into(),
            k: 10,
            decided_from: day(2023, 1, 1),
            decided_to: day(2023, 12, 31),
            field: Some(vec!["administrative".into()]),
            ..CaseQuery::default()
        },
    )
    .await?;
    check(
        dated.hits.len() == 1 && dated.hits[0].decision_id == 11,
        format!("date and field filters, got {:?}", dated.hits),
    )?;
    Ok(())
}

#[tokio::test]
async fn case_search_filters_skips_bundles_and_returns_matching_chunk() -> Checked {
    with_fixture(|fx| async move {
        let r = case_search_checks(&fx).await;
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
            &CaseQuery {
                text: "보상 규모".into(),
                k: 5,
                ..CaseQuery::default()
            },
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
