use sqlx::{AssertSqlSafe, PgPool, Row};

use super::{META_RUN, overruled_by};
use crate::error::RetrievalError;
use crate::model::CaseHit;

#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub section_id: i64,
    pub chunk_id: i64,
    pub position: i32,
    pub score: f64,
    pub matched: String,
}

fn details_sql() -> String {
    format!(
        "SELECT d.decision_id, c.name, d.case_no_display, d.decided_on, d.field, s.role, \
           CASE WHEN k.chunk_id IS NULL THEN s.body \
                ELSE substring(s.body FROM k.char_start + 1 FOR k.char_end - k.char_start) END, p.ord, \
           d.en_banc, d.disposition, s.opinion, s.direction, {overruled}, \
           EXISTS (SELECT 1 FROM section_span sp \
             WHERE sp.section_id = s.section_id AND sp.kind = 'lower_court_view' AND sp.run_id IN {META_RUN} \
               AND (k.chunk_id IS NULL OR (sp.char_start < k.char_end AND sp.char_end > k.char_start))) \
         FROM unnest($1::int8[], $2::int8[], $3::int4[]) WITH ORDINALITY AS p(section_id, chunk_id, pos, ord) \
         JOIN decision_section s ON s.section_id = p.section_id \
         JOIN decision d ON d.decision_id = s.decision_id \
         JOIN court c ON c.court_id = d.court_id \
         LEFT JOIN LATERAL ( \
           SELECT ch.chunk_id, ch.char_start, ch.char_end FROM decision_chunk ch \
           WHERE ch.section_id = s.section_id \
           ORDER BY (ch.chunk_id = p.chunk_id) DESC, \
                    (p.pos > 0 AND ch.char_start < p.pos AND ch.char_end >= p.pos) DESC, ch.ord \
           LIMIT 1 \
         ) k ON true \
         ORDER BY p.ord",
        overruled = overruled_by("d.decision_id"),
    )
}

pub async fn hits(pool: &PgPool, picks: &[Pick]) -> Result<Vec<CaseHit>, RetrievalError> {
    let sections: Vec<i64> = picks.iter().map(|p| p.section_id).collect();
    let chunks: Vec<i64> = picks.iter().map(|p| p.chunk_id).collect();
    let positions: Vec<i32> = picks.iter().map(|p| p.position).collect();
    let rows = sqlx::query(AssertSqlSafe(details_sql()))
        .bind(&sections)
        .bind(&chunks)
        .bind(&positions)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let ord: i64 = row.try_get(7)?;
        let pick = usize::try_from(ord - 1).ok().and_then(|i| picks.get(i));
        out.push(CaseHit {
            decision_id: row.try_get(0)?,
            court: row.try_get(1)?,
            case_no: row.try_get(2)?,
            decided_on: row.try_get(3)?,
            field: row.try_get(4)?,
            section_role: row.try_get(5)?,
            text: row.try_get(6)?,
            score: pick.map(|p| p.score).unwrap_or(0.0),
            en_banc: row.try_get(8)?,
            disposition: row.try_get(9)?,
            opinion: row.try_get(10)?,
            direction: row.try_get(11)?,
            overruled_by: row.try_get(12)?,
            lower_court_view: row.try_get(13)?,
            matched: pick.map(|p| p.matched.clone()).unwrap_or_default(),
        });
    }
    Ok(out)
}
