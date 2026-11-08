mod common;
mod fixture;

use std::sync::Arc;

use common::{corpus, with_fixture};
use fixture::{Checked, Fixture, check};
use retrieval::{CitationItem, CitationsBy, NoEmbedder, RetrievalError};

async fn read_checks(fx: &Fixture) -> Checked {
    let c = corpus(fx, Arc::new(NoEmbedder))?;
    let pool = c.pool();
    let case = retrieval::read_case(pool, 10, None).await?;
    check(case.court == "대법원" && case.case_no == "2020다1111", "case head")?;
    check(case.decided_on.to_string() == "2021-03-04", "decided date")?;
    check(case.sections.len() == 2, "all sections")?;
    check(case.sections[0].role == "holding_summary", "section order")?;
    check(case.sections[0].heading == "판결요지", "heading")?;

    let by_label = retrieval::read_case(pool, 10, Some(&["이유".to_string()])).await?;
    check(
        by_label.sections.len() == 1 && by_label.sections[0].role == "reasons",
        "filter by label",
    )?;
    let by_code = retrieval::read_case(pool, 10, Some(&["holding_summary".to_string()])).await?;
    check(by_code.sections.len() == 1, "filter by code")?;
    let bundle = retrieval::read_case(pool, 12, None).await?;
    check(bundle.sections[0].heading.is_empty(), "missing heading is empty")?;
    let none = retrieval::read_case(pool, 999, None).await;
    check(
        matches!(none, Err(RetrievalError::NotFound(_))),
        "missing case is NotFound",
    )?;
    Ok(())
}

#[tokio::test]
async fn read_case_follows_section_filters() -> Checked {
    with_fixture(|fx| async move {
        let r = read_checks(&fx).await;
        (fx, r)
    })
    .await
}

async fn citation_checks(fx: &Fixture) -> Checked {
    let c = corpus(fx, Arc::new(NoEmbedder))?;
    let pool = c.pool();
    let cited = retrieval::citations(pool, &CitationsBy::Decision(10), 50).await?;
    check(cited.extract_run.as_deref() == Some("2:v2"), "active citation run")?;
    check(cited.items.len() == 2, format!("cited units {:?}", cited.items))?;
    let applied = cited
        .items
        .iter()
        .find_map(|i| match i {
            CitationItem::Cited(u) if u.relation == "applies" => Some(u),
            _ => None,
        })
        .ok_or("applied citation expected")?;
    check(applied.law_name.as_deref() == Some("국가배상법"), "law name")?;
    check(applied.unit_key.as_deref() == Some("제2조"), "unit path")?;
    check(applied.lsi_seq == Some(fixture::CURRENT_SEQ), "version of cited unit")?;

    let other = retrieval::citations(pool, &CitationsBy::Decision(11), 50).await?;
    check(other.items.len() == 1, "inactive run rows are not read")?;
    let json = serde_json::to_value(&other)?;
    check(
        json["items"][0].get("lsi_seq").is_none(),
        "missing version is left out",
    )?;

    let citing = retrieval::citations(
        pool,
        &CitationsBy::LawUnit {
            law: "국가배상법".into(),
            unit: "제2조".into(),
        },
        50,
    )
    .await?;
    let ids: Vec<i64> = citing
        .items
        .iter()
        .filter_map(|i| match i {
            CitationItem::Citing(c) => Some(c.decision_id),
            CitationItem::Cited(_) => None,
        })
        .collect();
    check(ids == vec![11, 10], format!("citing decisions newest first {ids:?}"))?;

    let limited = retrieval::citations(
        pool,
        &CitationsBy::LawUnit {
            law: "국가배상법".into(),
            unit: "제2조".into(),
        },
        1,
    )
    .await?;
    check(limited.items.len() == 1, "limit")?;

    let art3 = retrieval::citations(
        pool,
        &CitationsBy::LawUnit {
            law: "국가배상법".into(),
            unit: "제3조".into(),
        },
        50,
    )
    .await?;
    check(art3.items.is_empty(), "only the inactive run cited 제3조")?;
    Ok(())
}

#[tokio::test]
async fn citations_read_only_the_active_run_in_both_directions() -> Checked {
    with_fixture(|fx| async move {
        let r = citation_checks(&fx).await;
        (fx, r)
    })
    .await
}

async fn read_only_checks(fx: &Fixture) -> Checked {
    let c = corpus(fx, Arc::new(NoEmbedder))?;
    let write = sqlx::query("DELETE FROM law_unit").execute(c.pool()).await;
    check(write.is_err(), "corpus pool must refuse writes")?;
    Ok(())
}

#[tokio::test]
async fn corpus_pool_is_read_only() -> Checked {
    with_fixture(|fx| async move {
        let r = read_only_checks(&fx).await;
        (fx, r)
    })
    .await
}
