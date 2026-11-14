use std::collections::HashMap;

use sqlx::{PgPool, Row};

use super::laws::UnitRef;
use crate::error::RetrievalError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LawWeights {
    pub statute: f64,
    pub decree: f64,
    pub rule: f64,
    pub admin_rule: f64,
    pub annexes: bool,
}

impl Default for LawWeights {
    fn default() -> Self {
        Self { statute: 1.0, decree: 1.0, rule: 1.0, admin_rule: 1.0, annexes: true }
    }
}

impl LawWeights {
    pub fn parse(text: &str) -> Option<Self> {
        let v: Vec<f64> = text.split(',').map(|s| s.trim().parse::<f64>()).collect::<Result<_, _>>().ok()?;
        let [statute, decree, rule, admin_rule] = v.as_slice() else { return None };
        let all = [*statute, *decree, *rule, *admin_rule];
        if all.iter().any(|w| !w.is_finite() || *w <= 0.0 || *w > 1.0) {
            return None;
        }
        Some(Self { statute: *statute, decree: *decree, rule: *rule, admin_rule: *admin_rule, annexes: true })
    }

    pub fn articles_only(mut self) -> Self {
        self.annexes = false;
        self
    }

    pub fn is_flat(&self) -> bool {
        self.statute == 1.0 && self.decree == 1.0 && self.rule == 1.0 && self.admin_rule == 1.0
    }

    pub fn annex_sql(&self, kind: &str, law_type: &str) -> String {
        if self.annexes { self.sql(kind, law_type) } else { "1.0".to_string() }
    }

    fn value(&self, slot: norms::Weight) -> f64 {
        match slot {
            norms::Weight::Statute => self.statute,
            norms::Weight::Decree => self.decree,
            norms::Weight::Rule | norms::Weight::ByLawType => self.rule,
            norms::Weight::AdminRule => self.admin_rule,
        }
    }

    pub fn sql(&self, kind: &str, law_type: &str) -> String {
        let mut arms = vec![
            format!("WHEN {kind} = 'law' AND {law_type} = '법률' THEN {:.4}", self.statute),
            format!("WHEN {kind} = 'law' AND {law_type} = '대통령령' THEN {:.4}", self.decree),
            format!("WHEN {kind} = 'law' THEN {:.4}", self.rule),
        ];
        for k in norms::Kind::ALL.into_iter().filter(|k| *k != norms::Kind::Law) {
            arms.push(format!("WHEN {kind} = '{}' THEN {:.4}", k.db_value(), self.value(k.norm().weight())));
        }
        format!("(CASE {} ELSE {:.4} END)", arms.join(" "), self.admin_rule)
    }

    pub fn of(&self, kind: &str, law_type: &str) -> f64 {
        norms::Kind::from_db(kind).map_or(self.admin_rule, |k| self.value(norms::weight_slot(k, law_type)))
    }
}

pub async fn unit_weights(
    pool: &PgPool,
    units: &[UnitRef],
    weights: &LawWeights,
) -> Result<HashMap<UnitRef, f64>, RetrievalError> {
    let mut arts = Vec::new();
    let mut annexes = Vec::new();
    let mut nodes = Vec::new();
    for u in units {
        match u {
            UnitRef::Article(id) => arts.push(*id),
            UnitRef::Annex(id) => annexes.push(*id),
            UnitRef::Node(id) => nodes.push(id.clone()),
        }
    }
    let rows = sqlx::query(
        "SELECT 'a' AS src, av.row_id::text AS id, l.kind, coalesce(l.law_type, '') AS law_type FROM article_version av \
           JOIN law l ON l.law_id = av.law_id WHERE av.row_id = ANY($1) \
         UNION ALL \
         SELECT 'x', a.row_id::text, l.kind, coalesce(l.law_type, '') FROM annex a \
           JOIN law l ON l.law_id = a.law_id WHERE a.row_id = ANY($2) \
         UNION ALL \
         SELECT 'n', n.node_id, l.kind, coalesce(l.law_type, '') FROM annex_node n JOIN annex a ON a.row_id = n.annex_row_id \
           JOIN law l ON l.law_id = a.law_id WHERE n.node_id = ANY($3)",
    )
    .bind(&arts)
    .bind(&annexes)
    .bind(&nodes)
    .fetch_all(pool)
    .await?;
    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        let src: String = row.try_get(0)?;
        let id: String = row.try_get(1)?;
        let kind: String = row.try_get(2)?;
        let law_type: String = row.try_get(3)?;
        let unit = match src.as_str() {
            "a" => UnitRef::Article(id.parse().map_err(|_| RetrievalError::Argument("weight id"))?),
            "x" => UnitRef::Annex(id.parse().map_err(|_| RetrievalError::Argument("weight id"))?),
            _ => UnitRef::Node(id),
        };
        let w = if src == "a" || weights.annexes { weights.of(&kind, &law_type) } else { 1.0 };
        out.insert(unit, w);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::LawWeights;

    #[test]
    fn weights_parse_and_map_law_kinds() {
        let parsed = LawWeights::parse("1, 0.9,0.8,0.5");
        assert!(parsed.is_some());
        let w = parsed.unwrap_or_default();
        assert_eq!(w.of("law", "법률"), 1.0);
        assert_eq!(w.of("law", "대통령령"), 0.9);
        assert_eq!(w.of("law", "부령"), 0.8);
        assert_eq!(w.of("law", "대법원규칙"), 0.8);
        assert_eq!(w.of("admrul", "고시"), 0.5);
        assert_eq!(w.of("rule", "학칙"), 0.8);
        assert_eq!(w.of("ordin", "조례"), 0.8);
        assert_eq!(w.of("mystery", ""), 0.5);
        let sql = w.sql("l.kind", "l.law_type");
        assert!(sql.contains("WHEN l.kind = 'rule' THEN 0.8000") && sql.contains("WHEN l.kind = 'ordin' THEN 0.8000") && sql.contains("WHEN l.kind = 'admrul' THEN 0.5000"));
        assert!(LawWeights::parse("1,0.9,0.8").is_none());
        assert!(LawWeights::parse("1,0.9,0.8,0").is_none());
        assert!(LawWeights::parse("1,0.9,0.8,x").is_none());
        assert!(LawWeights::default().is_flat());
        let sql = w.sql("l.kind", "l.law_type");
        assert!(sql.contains("THEN 1.0000") && sql.contains("THEN 0.9000") && sql.contains("ELSE 0.5000"));
    }
}
