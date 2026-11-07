mod fixture;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::NaiveDate;
use fixture::{Checked, Fixture, check};
use retrieval::{
    CaseQuery, CitationItem, CitationsBy, Corpus, CorpusDb, DIM, EmbedFuture, Embedder, LawQuery,
    NoEmbedder, RetrievalError,
};

struct FixedEmbedder {
    calls: AtomicUsize,
}

impl Embedder for FixedEmbedder {
    fn registry_name(&self) -> &str {
        "registry-model"
    }

    fn embed<'a>(&'a self, _text: &'a str) -> EmbedFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(vec![0.01_f32; DIM]) })
    }
}

fn corpus(fx: &Fixture, embedder: Arc<dyn Embedder>) -> Result<Corpus, RetrievalError> {
    let pool = CorpusDb::parse(&fx.url)?.connect_lazy(4, Duration::from_secs(10));
    Ok(Corpus::new(pool, embedder))
}

fn day(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y, m, d)
}

async fn with_fixture<F, Fut>(body: F) -> Checked
where
    F: FnOnce(Fixture) -> Fut,
    Fut: Future<Output = (Fixture, Checked)>,
{
    let Some(fx) = fixture::open().await? else {
        return Ok(());
    };
    let (fx, result) = body(fx).await;
    let schema = fx.schema.clone();
    fx.close().await?;
    if let Ok(base) = std::env::var(fixture::URL_VAR) {
        check(
            !Fixture::schema_exists(&base, &schema).await?,
            "temporary schema was not dropped",
        )?;
    }
    result
}

fn law_query(text: &str) -> LawQuery {
    LawQuery {
        text: text.into(),
        k: 8,
        ..LawQuery::default()
    }
}

async fn law_search_checks(fx: &Fixture) -> Checked {
    let c = corpus(fx, Arc::new(NoEmbedder))?;
    let r = retrieval::search_laws(&c, &law_query("공무원 직무 과실 손해")).await?;
    check(r.degraded, "embedding failure must mark the result degraded")?;
    let first = r.hits.first().ok_or("no law hits")?;
    check(first.law_name == "국가배상법", format!("first hit {first:?}"))?;
    check(first.unit_key == "제2조", format!("first hit {first:?}"))?;
    check(first.lsi_seq == fixture::CURRENT_SEQ, "current version expected")?;
    check(first.text == fixture::ART2_NOW, "body text expected")?;
    check(first.score > 0.0, "score must be positive")?;
    check(
        r.hits.iter().all(|h| h.lsi_seq != fixture::OLD_SEQ && h.lsi_seq != fixture::SCHEDULED_SEQ),
        "only the current version may appear without as_of",
    )?;
    check(
        r.hits.iter().all(|h| h.unit_key != "제4조"),
        "deleted unit must not appear",
    )?;
    let json = serde_json::to_value(&r)?;
    let keys: Vec<String> = json["hits"][0]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    let mut want = vec![
        "enforcement_date", "kind", "law_name", "lsi_seq", "score", "text", "title", "unit_key",
    ];
    want.sort_unstable();
    let mut got: Vec<&str> = keys.iter().map(String::as_str).collect();
    got.sort_unstable();
    check(got == want, format!("hit fields {got:?}"))?;
    check(json["degraded"] == true, "degraded flag in JSON")?;

    let older = LawQuery {
        as_of: day(2020, 6, 1),
        ..law_query("공무원 고의 손해")
    };
    let r = retrieval::search_laws(&c, &older).await?;
    let first = r.hits.first().ok_or("no hits for as_of")?;
    check(first.lsi_seq == fixture::OLD_SEQ, format!("as_of picked {first:?}"))?;
    check(
        r.hits.iter().all(|h| h.law_name != "국가배상법 시행령"),
        "decree version starts after as_of",
    )?;

    let decree_only = LawQuery {
        law_type: Some(vec!["대통령령".into()]),
        ..law_query("공무원 과실")
    };
    let r = retrieval::search_laws(&c, &decree_only).await?;
    check(!r.hits.is_empty(), "decree hit expected")?;
    check(
        r.hits.iter().all(|h| h.law_name == "국가배상법 시행령"),
        "law_type filter",
    )?;

    let few = LawQuery {
        k: 1,
        ..law_query("공무원 과실 배상")
    };
    let r = retrieval::search_laws(&c, &few).await?;
    check(r.hits.len() == 1, "k caps the hit count")?;

    let blank = retrieval::search_laws(&c, &law_query("   ")).await;
    check(
        matches!(blank, Err(RetrievalError::Argument(_))),
        "blank query is an argument error",
    )?;
    Ok(())
}

#[tokio::test]
async fn law_search_text_path_filters_versions_and_marks_degraded() -> Checked {
    with_fixture(|fx| async move {
        let r = law_search_checks(&fx).await;
        (fx, r)
    })
    .await
}

async fn vector_failure_checks(fx: &Fixture) -> Checked {
    let embedder = Arc::new(FixedEmbedder {
        calls: AtomicUsize::new(0),
    });
    let c = corpus(fx, embedder.clone())?;
    let laws = retrieval::search_laws(&c, &law_query("공무원 과실")).await?;
    check(laws.degraded, "vector query failure must degrade, not fail")?;
    check(!laws.hits.is_empty(), "text hits still returned")?;
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
        embedder.calls.load(Ordering::SeqCst) == 2,
        "embedder called once per search",
    )?;
    Ok(())
}

#[tokio::test]
async fn vector_path_with_fake_embedder_degrades_when_vector_tables_are_absent() -> Checked {
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
