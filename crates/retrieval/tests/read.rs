mod common;
mod fixture;

use std::sync::Arc;

use common::{corpus, day, with_fixture};
use fixture::{Checked, Fixture, check};
use retrieval::{CitationItem, CitationsBy, NoEmbedder, RetrievalError};

async fn read_checks(fx: &Fixture) -> Checked {
    let c = corpus(fx, Arc::new(NoEmbedder))?;
    let pool = c.pool();
    let now = retrieval::read_law_unit(pool, "국가배상법", "제 2 조", None).await?;
    check(now.body == fixture::ART2_NOW, "current body")?;
    check(now.version.lsi_seq == fixture::CURRENT_SEQ, "current version")?;
    check(now.version.status == "current", "status")?;
    check(now.enforcement_date == day(2023, 1, 1), "version enforcement date")?;
    check(now.title == "배상책임", "title")?;

    let old = retrieval::read_law_unit(pool, "국가배상법", "제2조", day(2019, 1, 1)).await?;
    check(old.body == fixture::ART2_OLD, "as_of body")?;
    check(old.version.status == "past", "as_of status")?;

    let art3 = retrieval::read_law_unit(pool, "국가배상법", "제3조", None).await?;
    check(art3.enforcement_date == day(2023, 7, 1), "unit enforcement date wins")?;
    check(art3.title.is_empty(), "missing title is empty")?;

    let decree = retrieval::read_law_unit(pool, "국가배상법시행령", "제3조", None).await?;
    check(decree.law_name == "국가배상법 시행령", "law name with spaces removed")?;

    let annex = retrieval::read_law_unit(pool, "L1", "별표1", None).await?;
    check(annex.content_status == "link_only", "annex content status")?;
    check(
        annex.source_urls == serde_json::json!(["https://example.invalid/annex-1.pdf"]),
        "source urls",
    )?;
    let json = serde_json::to_value(&annex)?;
    check(json["version"]["kind"] == "law", "version object")?;

    let deleted = retrieval::read_law_unit(pool, "국가배상법", "제4조", None).await?;
    check(deleted.deleted, "deleted flag")?;

    let missing = retrieval::read_law_unit(pool, "국가배상법", "제99조", None).await;
    check(
        matches!(missing, Err(RetrievalError::NotFound(_))),
        "missing unit is NotFound",
    )?;
    let before = retrieval::read_law_unit(pool, "국가배상법", "제2조", day(2010, 1, 1)).await;
    check(
        matches!(before, Err(RetrievalError::NotFound(_))),
        "no version before the first enforcement date",
    )?;

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
async fn read_law_unit_and_read_case_follow_versions_and_section_filters() -> Checked {
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
