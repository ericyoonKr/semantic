mod common;
mod fixture;

use std::sync::Arc;

use common::{corpus, with_fixture};
use fixture::{Checked, Fixture, check};
use retrieval::{CitationItem, Citations, CitationsBy, NoEmbedder, RetrievalError};

async fn read_checks(fx: &Fixture) -> Checked {
    let c = corpus(fx, Arc::new(NoEmbedder))?;
    let pool = c.pool();
    let case = retrieval::read_case(pool, 10, None, false).await?;
    check(case.court == "대법원" && case.case_no == "2020다1111", "case head")?;
    check(case.decided_on.to_string() == "2021-03-04", "decided date")?;
    check(case.en_banc && case.disposition == "reverse_remand", format!("marks {case:?}"))?;
    check(case.overruled_by.is_empty(), "inactive overrule run is not read")?;
    let roles: Vec<&str> = case.sections.iter().map(|s| s.role.as_str()).collect();
    check(roles == ["holding_issue", "holding_summary", "reasons", "ref_statutes"], format!("majority sections {roles:?}"))?;
    check(case.sections[0].direction.as_deref() == Some("negative"), "holding issue direction")?;
    check(case.sections[1].opinion.as_deref() == Some("majority"), "opinion is shown")?;
    let head = fixture::REASONS_HEAD.chars().count();
    let width = i32::try_from(head)?;
    check(case.sections[2].lower_court_view == vec![[0, width]], format!("active span only {:?}", case.sections[2]))?;

    let all = retrieval::read_case(pool, 10, None, true).await?;
    let dissent = all.sections.iter().find(|s| s.opinion.as_deref() == Some("dissent")).ok_or("dissent expected")?;
    check(dissent.body == fixture::DISSENT_10 && all.sections.len() == 5, "minority sections when asked")?;

    let by_label = retrieval::read_case(pool, 10, Some(&["이유".to_string()]), false).await?;
    check(by_label.sections.len() == 1 && by_label.sections[0].role == "reasons", "filter by label")?;
    let by_code = retrieval::read_case(pool, 10, Some(&["holding_summary".to_string()]), false).await?;
    check(by_code.sections.len() == 1, "filter by code")?;
    let bundle = retrieval::read_case(pool, 12, None, false).await?;
    check(bundle.sections[0].heading.is_empty(), "missing heading is empty")?;
    let old = retrieval::read_case(pool, 13, None, false).await?;
    check(old.overruled_by == ["2020다1111"], format!("overruled by {:?}", old.overruled_by))?;
    let json = serde_json::to_value(&case)?;
    check(json.get("overruled_by").is_none(), "empty overrule list is left out")?;
    let none = retrieval::read_case(pool, 999, None, false).await;
    check(matches!(none, Err(RetrievalError::NotFound(_))), "missing case is NotFound")?;
    Ok(())
}

#[tokio::test]
async fn read_case_shows_majority_marks_and_overrules() -> Checked {
    with_fixture(|fx| async move {
        let r = read_checks(&fx).await;
        (fx, r)
    })
    .await
}

fn citing_ids(c: &Citations) -> Vec<i64> {
    c.items
        .iter()
        .filter_map(|i| match i {
            CitationItem::Citing(c) => Some(c.decision_id),
            CitationItem::Cited(_) => None,
        })
        .collect()
}

fn unit(law: &str, unit: &str) -> CitationsBy {
    CitationsBy::LawUnit { law: law.into(), unit: unit.into() }
}

async fn citation_checks(fx: &Fixture) -> Checked {
    let c = corpus(fx, Arc::new(NoEmbedder))?;
    let pool = c.pool();
    let cited = retrieval::citations(pool, &CitationsBy::Decision(10), 50).await?;
    check(cited.extract_run.as_deref() == Some("2:v2"), "active citation run")?;
    check(cited.items.len() == 2, format!("cited units {:?}", cited.items))?;
    let CitationItem::Cited(first) = &cited.items[0] else {
        return Err("cited unit expected".into());
    };
    check(first.law_name.as_deref() == Some("국가배상법"), "law name")?;
    check(first.unit_key.as_deref() == Some("제2조"), "unit path")?;
    check(first.article_key.as_deref() == Some("L1:0002"), "article key")?;
    check(first.article_version_id.as_deref() == Some("L1:0002@2017-01-01"), "linked version")?;

    let other = retrieval::citations(pool, &CitationsBy::Decision(11), 50).await?;
    check(other.items.len() == 1, "inactive run rows are not read")?;
    let json = serde_json::to_value(&other)?;
    check(json["items"][0].get("article_version_id").is_none(), "missing version is left out")?;
    let unknown = retrieval::citations(pool, &CitationsBy::Decision(13), 50).await?;
    let json = serde_json::to_value(&unknown)?;
    check(json["items"][1].get("law_name").is_none() && json["items"][1]["raw"] == "없는법 제9조", format!("{json}"))?;

    let citing = retrieval::citations(pool, &unit("국가배상법", "제2조"), 50).await?;
    check(citing_ids(&citing) == vec![11, 10, 13], format!("citing decisions newest first {:?}", citing_ids(&citing)))?;
    let by_id = retrieval::citations(pool, &unit("L1", "제 2 조"), 50).await?;
    check(citing_ids(&by_id) == vec![11, 10, 13], "law id and spaced unit")?;
    let paragraph = retrieval::citations(pool, &unit("국가배상법", "제2조 제1항"), 50).await?;
    check(citing_ids(&paragraph) == vec![11], format!("제1항 does not take 제11항 {:?}", citing_ids(&paragraph)))?;
    let CitationItem::Citing(row) = &citing.items[1] else {
        return Err("citing case expected".into());
    };
    check(row.article_version_id.as_deref() == Some("L1:0002@2017-01-01"), "citing row carries the version")?;
    let limited = retrieval::citations(pool, &unit("국가배상법", "제2조"), 1).await?;
    check(limited.items.len() == 1, "limit")?;
    let art3 = retrieval::citations(pool, &unit("국가배상법", "제3조"), 50).await?;
    check(art3.items.is_empty(), "only the inactive run cited 제3조")?;
    let annex = retrieval::citations(pool, &unit("국가배상법", "별표 1"), 50).await;
    check(matches!(annex, Err(RetrievalError::Argument(_))), "unit without an article number")?;
    let nolaw = retrieval::citations(pool, &unit("없는법", "제2조"), 50).await?;
    check(nolaw.items.is_empty(), "unknown law gives nothing")?;
    Ok(())
}

#[tokio::test]
async fn citations_read_the_new_table_in_both_directions() -> Checked {
    with_fixture(|fx| async move {
        let r = citation_checks(&fx).await;
        (fx, r)
    })
    .await
}

async fn read_only_checks(fx: &Fixture) -> Checked {
    let c = corpus(fx, Arc::new(NoEmbedder))?;
    let write = sqlx::query("DELETE FROM decision_citation").execute(c.pool()).await;
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
