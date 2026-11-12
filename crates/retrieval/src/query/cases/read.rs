use chrono::NaiveDate;
use sqlx::{AssertSqlSafe, PgPool, Row};

use super::{META_RUN, opinion_allowed, overruled_by};
use crate::error::RetrievalError;
use crate::model::{CaseDoc, CaseSection};

fn head_sql() -> String {
    format!(
        "SELECT c.name, d.case_no_display, d.decided_on, d.field, d.en_banc, d.disposition, {overruled} \
         FROM decision d JOIN court c ON c.court_id = d.court_id WHERE d.decision_id = $1",
        overruled = overruled_by("d.decision_id"),
    )
}

fn sections_sql() -> String {
    format!(
        "SELECT s.role, COALESCE(s.heading, ''), s.body, s.opinion, s.direction, \
           ARRAY(SELECT sp.char_start FROM section_span sp WHERE sp.section_id = s.section_id \
                   AND sp.kind = 'lower_court_view' AND sp.run_id IN {META_RUN} ORDER BY sp.char_start, sp.span_id), \
           ARRAY(SELECT sp.char_end FROM section_span sp WHERE sp.section_id = s.section_id \
                   AND sp.kind = 'lower_court_view' AND sp.run_id IN {META_RUN} ORDER BY sp.char_start, sp.span_id) \
         FROM decision_section s \
         LEFT JOIN section_role r ON r.role = s.role \
         WHERE s.decision_id = $1 AND ($2::text[] IS NULL OR s.role = ANY($2::text[]) OR r.label = ANY($2::text[])) \
           AND {opinion} \
         ORDER BY s.ord",
        opinion = opinion_allowed(3),
    )
}

type Head = (String, String, NaiveDate, String, bool, String, Vec<String>);

pub async fn read_case(
    pool: &PgPool,
    decision_id: i64,
    sections: Option<&[String]>,
    include_minority: bool,
) -> Result<CaseDoc, RetrievalError> {
    let head: Option<Head> = sqlx::query_as(AssertSqlSafe(head_sql())).bind(decision_id).fetch_optional(pool).await?;
    let (court, case_no, decided_on, field, en_banc, disposition, overruled_by) =
        head.ok_or(RetrievalError::NotFound("decision"))?;
    let wanted: Option<Vec<String>> = sections.map(<[String]>::to_vec);
    let rows = sqlx::query(AssertSqlSafe(sections_sql()))
        .bind(decision_id)
        .bind(wanted)
        .bind(include_minority)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let starts: Vec<i32> = row.try_get(5)?;
        let ends: Vec<i32> = row.try_get(6)?;
        out.push(CaseSection {
            role: row.try_get(0)?,
            heading: row.try_get(1)?,
            body: row.try_get(2)?,
            opinion: row.try_get(3)?,
            direction: row.try_get(4)?,
            lower_court_view: starts.into_iter().zip(ends).map(|(a, b)| [a, b]).collect(),
        });
    }
    Ok(CaseDoc { court, case_no, decided_on, field, en_banc, disposition, overruled_by, sections: out })
}
