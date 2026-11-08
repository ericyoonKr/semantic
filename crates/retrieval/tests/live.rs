use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use retrieval::{
    AnnexCompute, AnnexLookup, Corpus, CorpusDb, LawQuery, NoEmbedder, RetrievalError,
};

type Checked = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const URL_VAR: &str = "CORPUS_PG_URL";
const FINE_LAW: &str = "산업기술의 유출방지 및 보호에 관한 법률 시행령";

fn corpus() -> Result<Option<Corpus>, RetrievalError> {
    let Ok(url) = std::env::var(URL_VAR) else {
        eprintln!("{URL_VAR} 없음 — 실제 법령 DB 시험을 건너뜀");
        return Ok(None);
    };
    let pool = CorpusDb::parse(&url)?.connect_lazy(4, Duration::from_secs(30));
    Ok(Some(Corpus::new(pool, Arc::new(NoEmbedder))))
}

fn check(ok: bool, what: impl std::fmt::Display) -> Checked {
    if ok { Ok(()) } else { Err(what.to_string().into()) }
}

fn day(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y, m, d)
}

#[tokio::test]
async fn article_is_read_by_law_and_number_at_the_given_date() -> Checked {
    let Some(c) = corpus()? else { return Ok(()) };
    let now = retrieval::read_law_unit(c.pool(), "지방공무원법", "제 25 조의 2", None).await?;
    check(now.title == "외국인과 복수국적자의 임용", &now.title)?;
    check(now.unit_kind == "article", &now.unit_kind)?;
    check(now.version.valid_to.is_none(), format!("{:?}", now.version))?;
    let old = retrieval::read_law_unit(c.pool(), "지방공무원법", "제25조의2", day(2022, 1, 1)).await?;
    check(old.version.version_id == "001653:0025021@2021-12-09", &old.version.version_id)?;
    check(old.version.valid_to == day(2022, 1, 13), format!("{:?}", old.version))?;
    let none = retrieval::read_law_unit(c.pool(), "지방공무원법", "제999조", None).await;
    check(matches!(none, Err(RetrievalError::NotFound(_))), "missing article")?;
    Ok(())
}

#[tokio::test]
async fn law_and_article_in_the_query_come_first_and_empty_vectors_are_reported() -> Checked {
    let Some(c) = corpus()? else { return Ok(()) };
    let q = LawQuery { text: "지방공무원법 제25조의2".into(), k: 5, ..LawQuery::default() };
    let r = retrieval::search_laws(&c, &q).await?;
    let top = r.hits.first().ok_or("no hits")?;
    check(top.matched == "direct" && top.unit_key == "제25조의2", format!("{top:?}"))?;
    check(top.law_name == "지방공무원법", &top.law_name)?;
    check(r.degraded && r.degraded_reason.is_some(), format!("{:?}", r.degraded_reason))?;
    let old = LawQuery { as_of: day(2022, 1, 1), ..q };
    let r = retrieval::search_laws(&c, &old).await?;
    let top = r.hits.first().ok_or("no hits")?;
    check(top.version_id == "001653:0025021@2021-12-09", &top.version_id)?;
    Ok(())
}

#[tokio::test]
async fn text_search_reaches_annex_rows() -> Checked {
    let Some(c) = corpus()? else { return Ok(()) };
    let q = LawQuery {
        text: "국가핵심기술 보유기관 등록 신청하지 않은 경우 과태료".into(),
        k: 10,
        ..LawQuery::default()
    };
    let r = retrieval::search_laws(&c, &q).await?;
    let annex = r.hits.iter().find(|h| h.unit_kind.starts_with("annex"));
    check(annex.is_some(), format!("{:?}", r.hits.iter().map(|h| (&h.law_name, &h.unit_key)).collect::<Vec<_>>()))?;
    Ok(())
}

#[tokio::test]
async fn annex_lookup_and_compute_read_the_rows_amounts() -> Checked {
    let Some(c) = corpus()? else { return Ok(()) };
    let q = AnnexLookup {
        law: FINE_LAW.into(),
        annex: "별표 2".into(),
        as_of: day(2025, 10, 2),
        words: Some("국가 핵심기술 보유기관 등록".into()),
        ..AnnexLookup::default()
    };
    let t = retrieval::annex_lookup(c.pool(), &q).await?;
    check(t.annex.version_id == "010430|별표|2|0@2025-10-01", &t.annex.version_id)?;
    check(t.rows_matched == 1, format!("{} rows", t.rows_matched))?;
    let row = &t.rows[0];
    check(row.label.as_deref() == Some("나"), format!("{:?}", row.label))?;
    let column = Some("2차".to_string());
    let only = retrieval::annex_lookup(c.pool(), &AnnexLookup { column: column.clone(), ..q.clone() }).await?;
    check(only.rows[0].amounts.as_object().map(|m| m.len()) == Some(1), "column filter")?;
    let out = retrieval::annex_compute(
        c.pool(),
        &AnnexCompute { node_id: row.node_id.clone(), column: None, elapsed: None },
    )
    .await?;
    let krw: Vec<Option<i64>> = out.results.iter().map(|r| r.krw).collect();
    check(krw == vec![Some(5_000_000), Some(7_000_000)], format!("{krw:?}"))?;
    check(!out.notes.is_empty(), "notes")?;
    let missing = retrieval::annex_lookup(c.pool(), &AnnexLookup { annex: "별표 99".into(), ..q }).await;
    check(matches!(missing, Err(RetrievalError::NotFound(_))), "missing annex")?;
    Ok(())
}

#[tokio::test]
async fn abbreviation_reads_the_official_law() -> Checked {
    let Some(c) = corpus()? else { return Ok(()) };
    let Ok(path) = std::env::var("LAW_ALIASES") else {
        eprintln!("LAW_ALIASES 없음 — 약칭 시험을 건너뜀");
        return Ok(());
    };
    lawnames::install(lawnames::Aliases::load(std::path::Path::new(&path))?);
    let unit = retrieval::read_law_unit(c.pool(), "헌법", "제12조", None).await?;
    check(unit.law_name == "대한민국헌법", &unit.law_name)?;
    let abbr = retrieval::read_law_unit(c.pool(), "국토계획법", "제56조", None).await?;
    check(abbr.law_name == "국토의 계획 및 이용에 관한 법률", &abbr.law_name)?;
    let none = retrieval::read_law_unit(c.pool(), "구 헌법", "제12조", None).await;
    check(none.is_err(), "「구 헌법」은 약칭이 아님")?;
    Ok(())
}

#[tokio::test]
async fn vector_search_off_by_configuration_says_so() -> Checked {
    let Some(c) = corpus()? else { return Ok(()) };
    let c = c.without_vectors();
    let q = LawQuery { text: "영업정지 처분 기준".into(), k: 5, as_of: None, law_type: None };
    let r = retrieval::search_laws(&c, &q).await?;
    check(r.degraded, "degraded")?;
    check(r.degraded_reason.as_deref() == Some(retrieval::search::VECTOR_OFF), format!("{:?}", r.degraded_reason))?;
    Ok(())
}
