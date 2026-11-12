use std::collections::HashMap;

use sqlx::{AssertSqlSafe, PgPool};

use super::META_RUN;
use crate::error::RetrievalError;

pub const HOLDING_ROLES: [&str; 2] = ["holding_issue", "holding_summary"];
pub const DEFAULT_HOLDING: f64 = 1.2;
pub const DEFAULT_OVERRULED: f64 = 0.5;
const MAX_HOLDING: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaseWeights {
    pub holding: f64,
    pub overruled: f64,
}

impl Default for CaseWeights {
    fn default() -> Self {
        Self { holding: DEFAULT_HOLDING, overruled: DEFAULT_OVERRULED }
    }
}

impl CaseWeights {
    pub fn parse(text: &str) -> Option<Self> {
        let v: Vec<f64> = text.split(',').map(|s| s.trim().parse::<f64>()).collect::<Result<_, _>>().ok()?;
        let [holding, overruled] = v.as_slice() else { return None };
        let ok_holding = holding.is_finite() && (1.0..=MAX_HOLDING).contains(holding);
        let ok_overruled = overruled.is_finite() && *overruled > 0.0 && *overruled <= 1.0;
        (ok_holding && ok_overruled).then_some(Self { holding: *holding, overruled: *overruled })
    }

    pub fn of(&self, role: &str, overruled: bool) -> f64 {
        let r = if HOLDING_ROLES.contains(&role) { self.holding } else { 1.0 };
        let o = if overruled { self.overruled } else { 1.0 };
        r * o
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionFacts {
    pub role: String,
    pub overruled: bool,
}

fn facts_sql() -> String {
    format!(
        "SELECT s.section_id, s.role, EXISTS (SELECT 1 FROM decision_overrule o \
           WHERE o.overruled_decision_id = s.decision_id AND o.run_id IN {META_RUN}) \
         FROM decision_section s WHERE s.section_id = ANY($1)"
    )
}

pub async fn facts(pool: &PgPool, sections: &[i64]) -> Result<HashMap<i64, SectionFacts>, RetrievalError> {
    if sections.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(i64, String, bool)> =
        sqlx::query_as(AssertSqlSafe(facts_sql())).bind(sections).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(id, role, overruled)| (id, SectionFacts { role, overruled })).collect())
}

pub fn reweigh(fused: &mut [(i64, f64)], facts: &HashMap<i64, SectionFacts>, weights: &CaseWeights) {
    for (id, score) in fused.iter_mut() {
        if let Some(f) = facts.get(id) {
            *score *= weights.of(&f.role, f.overruled);
        }
    }
    fused.sort_by(|a, b| b.1.total_cmp(&a.1));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(role: &str, overruled: bool) -> SectionFacts {
        SectionFacts { role: role.into(), overruled }
    }

    #[test]
    fn weights_parse_with_bounds() {
        assert_eq!(CaseWeights::parse("1.5, 0.3"), Some(CaseWeights { holding: 1.5, overruled: 0.3 }));
        assert_eq!(CaseWeights::parse("1,1"), Some(CaseWeights { holding: 1.0, overruled: 1.0 }));
        assert!(CaseWeights::parse("0.9,0.5").is_none());
        assert!(CaseWeights::parse("1.2,0").is_none());
        assert!(CaseWeights::parse("1.2,1.5").is_none());
        assert!(CaseWeights::parse("6,0.5").is_none());
        assert!(CaseWeights::parse("1.2").is_none());
        assert!(CaseWeights::parse("x,0.5").is_none());
    }

    #[test]
    fn holding_sections_rise_and_overruled_decisions_sink() {
        let w = CaseWeights::default();
        let mut fused = vec![(1, 1.0 / 61.0), (2, 1.0 / 62.0), (3, 1.0 / 63.0), (4, 1.0 / 64.0)];
        let facts: HashMap<i64, SectionFacts> = [
            (1, fact("reasons", true)),
            (2, fact("reasons", false)),
            (3, fact("holding_summary", false)),
            (4, fact("holding_issue", true)),
        ]
        .into_iter()
        .collect();
        reweigh(&mut fused, &facts, &w);
        let order: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();
        assert_eq!(order, vec![3, 2, 4, 1]);
        assert!((fused[0].1 - 1.2 / 63.0).abs() < 1e-12);
        assert!((w.of("holding_issue", true) - 0.6).abs() < 1e-12);
    }
}
