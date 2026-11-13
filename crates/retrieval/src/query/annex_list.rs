use chrono::NaiveDate;
use serde::Serialize;
use sqlx::{AssertSqlSafe, PgPool};

use crate::error::RetrievalError;
use crate::query::annex::annex_label;
use crate::query::laws::{VALID_A, resolve_law};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnnexEntry {
    pub unit_key: String,
    pub annex_type: String,
    pub title: String,
    pub enforcement_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnnexList {
    pub law_id: String,
    pub law_name: String,
    pub annexes: Vec<AnnexEntry>,
}

fn list_sql() -> String {
    format!(
        "SELECT a.annex_type, a.annex_no, a.annex_sub_no, COALESCE(a.title, ''), a.valid_from FROM annex a \
         WHERE a.law_id = $2 AND upper_inf(a.sys_period) AND {VALID_A} \
         ORDER BY (a.annex_type <> '별표'), a.annex_type, a.annex_no, a.annex_sub_no"
    )
}

pub async fn list_annexes(pool: &PgPool, law: &str, as_of: Option<NaiveDate>) -> Result<Option<AnnexList>, RetrievalError> {
    let Some((law_id, law_name, _)) = resolve_law(pool, law).await? else {
        return Ok(None);
    };
    let rows: Vec<(String, i32, i32, String, Option<NaiveDate>)> = sqlx::query_as(AssertSqlSafe(list_sql()))
        .bind(as_of)
        .bind(&law_id)
        .fetch_all(pool)
        .await?;
    let mut annexes: Vec<AnnexEntry> = Vec::with_capacity(rows.len());
    for (annex_type, no, sub, title, from) in rows {
        let unit_key = annex_label(&annex_type, no, sub);
        if annexes.iter().any(|a| a.unit_key == unit_key) {
            continue;
        }
        annexes.push(AnnexEntry { unit_key, annex_type, title, enforcement_date: from });
    }
    Ok(Some(AnnexList { law_id, law_name, annexes }))
}

#[cfg(test)]
mod tests {
    use super::list_sql;

    #[test]
    fn lists_only_current_versions_valid_on_the_day_for_one_law() {
        let sql = list_sql();
        assert!(sql.contains("a.law_id = $2"));
        assert!(sql.contains("upper_inf(a.sys_period)"));
        assert!(sql.contains("COALESCE($1::date, current_date)"));
    }
}
