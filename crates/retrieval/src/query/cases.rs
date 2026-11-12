pub mod candidates;
pub mod cited;
pub mod hits;
pub mod rank;
pub mod read;

pub use read::read_case;

use crate::model::CaseQuery;
use crate::query::fusion::CANDIDATES;

pub const MAJORITY: &str = "majority";
pub const CITATION_RUN: &str = "(SELECT run_id FROM extract_run WHERE extractor = 'citation' AND is_active)";
pub const META_RUN: &str = "(SELECT run_id FROM extract_run WHERE extractor = 'case_meta' AND is_active)";

pub const DECISION_FILTERS: &str = "NOT d.is_bundle \
     AND ($1::text[] IS NULL OR d.field = ANY($1::text[])) \
     AND ($2::text[] IS NULL OR c.level = ANY($2::text[])) \
     AND ($3::date IS NULL OR d.decided_on >= $3::date) \
     AND ($4::date IS NULL OR d.decided_on <= $4::date)";

pub fn opinion_allowed(slot: usize) -> String {
    format!("(s.opinion IS NULL OR s.opinion = '{MAJORITY}' OR ${slot}::bool)")
}

pub fn overruled_by(decision: &str) -> String {
    format!(
        "ARRAY(SELECT od.case_no_display FROM decision od WHERE od.decision_id IN \
           (SELECT o.overruling_decision_id FROM decision_overrule o \
            WHERE o.overruled_decision_id = {decision} AND o.run_id IN {META_RUN}) \
         ORDER BY od.decided_on, od.decision_id)"
    )
}

pub fn filters() -> String {
    format!("{DECISION_FILTERS} AND {}", opinion_allowed(6))
}

pub(crate) fn bind_filters<'q, O>(
    query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments>,
    q: &CaseQuery,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments> {
    query
        .bind(q.field.clone())
        .bind(q.court_level.clone())
        .bind(q.decided_from)
        .bind(q.decided_to)
        .bind(CANDIDATES)
        .bind(q.include_minority)
}
