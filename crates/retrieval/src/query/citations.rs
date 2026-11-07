use chrono::NaiveDate;
use sqlx::PgPool;

use crate::error::RetrievalError;
use crate::model::{CitationItem, Citations, CitationsBy, CitedUnit, CitingCase};
use crate::query::terms::{compact, like_prefix};

const ACTIVE_RUN: &str =
    "SELECT run_id, version FROM extract_run WHERE extractor = 'citation' AND is_active ORDER BY run_id DESC LIMIT 1";

const CITED: &str = "SELECT l.name, li.unit_path, dc.relation, dc.raw, u.kind, u.lsi_seq \
     FROM decision_citation dc \
     LEFT JOIN law_unit_lineage li ON li.lineage_key = dc.lineage_key \
     LEFT JOIN law l ON l.law_id = li.law_id \
     LEFT JOIN law_unit u ON u.unit_id = dc.unit_id \
     WHERE dc.run_id = $1 AND dc.decision_id = $2 \
     ORDER BY dc.section_id, dc.char_start, dc.citation_id LIMIT $3";

const CITING: &str = "SELECT d.decision_id, c.name, d.case_no_display, d.decided_on, li.unit_path, dc.relation \
     FROM law l \
     JOIN law_unit_lineage li ON li.law_id = l.law_id \
     JOIN decision_citation dc ON dc.lineage_key = li.lineage_key AND dc.run_id = $1 \
     JOIN decision d ON d.decision_id = dc.decision_id \
     JOIN court c ON c.court_id = d.court_id \
     WHERE (l.law_id = $2 OR regexp_replace(l.name, '\\s', '', 'g') = $3) \
       AND (regexp_replace(li.unit_path, '\\s', '', 'g') = $4 \
            OR regexp_replace(li.unit_path, '\\s', '', 'g') LIKE $5) \
     ORDER BY d.decided_on DESC, d.decision_id, dc.citation_id LIMIT $6";

type CitedRow = (
    Option<String>,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<i64>,
);

type CitingRow = (i64, String, String, NaiveDate, String, String);

pub async fn citations(
    pool: &PgPool,
    by: &CitationsBy,
    limit: i64,
) -> Result<Citations, RetrievalError> {
    let run: Option<(i32, String)> = sqlx::query_as(ACTIVE_RUN).fetch_optional(pool).await?;
    let Some((run_id, version)) = run else {
        return Ok(Citations {
            items: Vec::new(),
            extract_run: None,
        });
    };
    let items = match by {
        CitationsBy::Decision(id) => {
            let rows: Vec<CitedRow> = sqlx::query_as(CITED)
                .bind(run_id)
                .bind(*id)
                .bind(limit)
                .fetch_all(pool)
                .await?;
            rows.into_iter()
                .map(|(law_name, unit_key, relation, raw, kind, lsi_seq)| {
                    CitationItem::Cited(CitedUnit {
                        law_name,
                        unit_key,
                        relation,
                        raw,
                        kind,
                        lsi_seq,
                    })
                })
                .collect()
        }
        CitationsBy::LawUnit { law, unit } => {
            let unit = compact(unit);
            if law.trim().is_empty() || unit.is_empty() {
                return Err(RetrievalError::Argument("law and unit"));
            }
            let children = format!("{}제%", like_prefix(&unit));
            let rows: Vec<CitingRow> = sqlx::query_as(CITING)
                .bind(run_id)
                .bind(law.trim())
                .bind(compact(law))
                .bind(&unit)
                .bind(children)
                .bind(limit)
                .fetch_all(pool)
                .await?;
            rows.into_iter()
                .map(
                    |(decision_id, court, case_no, decided_on, unit_key, relation)| {
                        CitationItem::Citing(CitingCase {
                            decision_id,
                            court,
                            case_no,
                            decided_on,
                            unit_key,
                            relation,
                        })
                    },
                )
                .collect()
        }
    };
    Ok(Citations {
        items,
        extract_run: Some(format!("{run_id}:{version}")),
    })
}
