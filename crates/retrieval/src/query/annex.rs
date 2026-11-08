use std::collections::HashMap;

use chrono::NaiveDate;
use serde_json::{Map, Value};
use sqlx::{AssertSqlSafe, PgPool, Row};

use crate::error::RetrievalError;
use crate::model::{
    AnnexCompute, AnnexLookup, AnnexPara, AnnexRow, AnnexRuleItem, AnnexTable, AnnexVersion,
    Computation, Computed, RelatedArticle,
};
use crate::query::laws::{VALID_A, VALID_AV, resolve_law};
use crate::query::terms::compact;

pub const LOOKUP_DEFAULT: usize = 50;
pub const LOOKUP_MAX: usize = 200;
const PARAS_MAX: usize = 120;

const FORM: &str = "별지서식";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnexName {
    pub annex_type: String,
    pub no: i32,
    pub sub: i32,
}

impl AnnexName {
    pub fn parse(text: &str) -> Option<Self> {
        let t: String = compact(text)
            .chars()
            .filter(|c| !matches!(c, '[' | ']'))
            .collect();
        let (kind, rest) = [("별지서식", FORM), ("별지", FORM), ("서식", FORM), ("별표", "별표"), ("별도", "별도")]
            .iter()
            .find_map(|(p, k)| t.strip_prefix(p).map(|r| (*k, r)))?;
        let no: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let rest = &rest[no.len()..];
        let sub = match rest.strip_prefix('의') {
            Some(b) if !b.is_empty() && b.chars().all(|c| c.is_ascii_digit()) => b.parse().ok()?,
            Some(_) => return None,
            None if rest.is_empty() => 0,
            None => return None,
        };
        Some(Self {
            annex_type: kind.to_string(),
            no: no.parse().ok()?,
            sub,
        })
    }

    pub fn key(&self, law_id: &str) -> String {
        format!("{law_id}|{}|{}|{}", self.annex_type, self.no, self.sub)
    }

    pub fn label(&self) -> String {
        annex_label(&self.annex_type, self.no, self.sub)
    }
}

pub fn annex_label(annex_type: &str, no: i32, sub: i32) -> String {
    if sub > 0 {
        format!("{annex_type} {no}의{sub}")
    } else {
        format!("{annex_type} {no}")
    }
}

struct Loaded {
    row_id: i64,
    version: AnnexVersion,
    columns: Vec<Vec<String>>,
    raw_text: String,
}

fn version_sql(by_key: bool) -> String {
    let filter = if by_key {
        format!("a.annex_key = $2 AND {VALID_A}")
    } else {
        "a.row_id = $2 AND ($1::date IS NULL OR a.valid_period @> $1::date)".to_string()
    };
    format!(
        "SELECT a.row_id, l.law_id, l.name, a.annex_key, a.annex_type, a.annex_no, a.annex_sub_no, a.annex_version_id, \
           a.title, a.title_related, lower(a.valid_period), upper(a.valid_period), a.end_reason, a.review_status, \
           a.source_pdf_url, a.source_hwp_url, a.source_image_url, a.grid_json, COALESCE(a.raw_text, '') \
         FROM annex a JOIN law l ON l.law_id = a.law_id \
         WHERE upper(a.sys_period) IS NULL AND {filter} ORDER BY a.row_id LIMIT 1"
    )
}

fn columns_of(grid: Option<Value>) -> Vec<Vec<String>> {
    grid.and_then(|g| g.get("tables").and_then(Value::as_array).cloned())
        .unwrap_or_default()
        .iter()
        .map(|t| {
            t.get("col_paths")
                .and_then(Value::as_array)
                .map(|c| c.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default()
        })
        .collect()
}

async fn load(
    pool: &PgPool,
    as_of: Option<NaiveDate>,
    by_key: Option<&str>,
    by_row: Option<i64>,
) -> Result<Option<Loaded>, RetrievalError> {
    let sql = version_sql(by_key.is_some());
    let query = sqlx::query(AssertSqlSafe(sql)).bind(as_of);
    let query = match (by_key, by_row) {
        (Some(k), _) => query.bind(k.to_string()),
        (None, Some(r)) => query.bind(r),
        (None, None) => return Ok(None),
    };
    let Some(row) = query.fetch_optional(pool).await? else {
        return Ok(None);
    };
    let annex_type: String = row.try_get(4)?;
    let urls: Vec<String> = [14, 15, 16]
        .iter()
        .filter_map(|i| row.try_get::<Option<String>, _>(*i).ok().flatten())
        .filter(|u| !u.is_empty())
        .collect();
    Ok(Some(Loaded {
        row_id: row.try_get(0)?,
        version: AnnexVersion {
            law_id: row.try_get(1)?,
            law_name: row.try_get(2)?,
            annex_key: row.try_get(3)?,
            annex_label: annex_label(&annex_type, row.try_get(5)?, row.try_get(6)?),
            annex_type,
            version_id: row.try_get(7)?,
            title: row.try_get(8)?,
            title_related: row.try_get(9)?,
            valid_from: row.try_get(10)?,
            valid_to: row.try_get(11)?,
            end_reason: row.try_get(12)?,
            review_status: row.try_get(13)?,
            source_urls: urls,
        },
        columns: columns_of(row.try_get(17)?),
        raw_text: row.try_get(18)?,
    }))
}

#[derive(Debug, Clone)]
struct Node {
    id: String,
    parent: Option<String>,
    path: String,
    node_type: String,
    ordinal: i32,
    label: Option<String>,
    text: String,
    cells: Option<Value>,
    amounts: Option<Value>,
    basis: Vec<String>,
}

async fn nodes(pool: &PgPool, row_id: i64) -> Result<Vec<Node>, RetrievalError> {
    let rows = sqlx::query(
        "SELECT node_id, parent_id, path::text, node_type, ordinal, label, text, cells, amounts, \
           COALESCE(basis_unit_ids, '{}'::text[]) FROM annex_node WHERE annex_row_id = $1",
    )
    .bind(row_id)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(Node {
            id: r.try_get(0)?,
            parent: r.try_get(1)?,
            path: r.try_get(2)?,
            node_type: r.try_get(3)?,
            ordinal: r.try_get(4)?,
            label: r.try_get(5)?,
            text: r.try_get(6)?,
            cells: r.try_get(7)?,
            amounts: r.try_get(8)?,
            basis: r.try_get(9)?,
        });
    }
    Ok(in_document_order(out))
}

fn in_document_order(mut nodes: Vec<Node>) -> Vec<Node> {
    let by_id: HashMap<String, (Option<String>, i32)> = nodes
        .iter()
        .map(|n| (n.id.clone(), (n.parent.clone(), n.ordinal)))
        .collect();
    let chain = |id: &str| {
        let mut out = Vec::new();
        let mut cur = Some(id.to_string());
        let mut guard = 0;
        while let Some(c) = cur {
            let Some((parent, ord)) = by_id.get(&c) else { break };
            out.push(*ord);
            cur = parent.clone();
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        out.reverse();
        out
    };
    let keys: HashMap<String, Vec<i32>> = nodes.iter().map(|n| (n.id.clone(), chain(&n.id))).collect();
    nodes.sort_by(|a, b| {
        keys.get(&a.id)
            .cmp(&keys.get(&b.id))
            .then_with(|| a.path.cmp(&b.path))
    });
    nodes
}

fn table_index(path: &str) -> Option<usize> {
    path.split('.').find_map(|seg| {
        let n = seg.strip_prefix('t')?;
        (!n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            .then(|| n.parse::<usize>().ok())
            .flatten()
            .and_then(|i| i.checked_sub(1))
    })
}

fn cell_texts(cells: &Option<Value>) -> String {
    cells
        .as_ref()
        .and_then(Value::as_object)
        .map(|m| {
            m.values()
                .filter_map(|v| v.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn row_matches(n: &Node, row: &str) -> bool {
    let want = compact(row);
    let want = want.trim_end_matches('.');
    if want.is_empty() {
        return true;
    }
    n.id == want
        || n.path == want
        || n.path.ends_with(&format!(".{want}"))
        || n.label.as_deref().map(|l| compact(l).trim_end_matches('.') == want).unwrap_or(false)
}

fn words_match(n: &Node, words: &[String]) -> bool {
    let hay = compact(&format!("{} {}", n.text, cell_texts(&n.cells)));
    words.iter().all(|w| hay.contains(w.as_str()))
}

fn keep_columns(amounts: &Option<Value>, column: Option<&str>) -> Value {
    let Some(map) = amounts.as_ref().and_then(Value::as_object) else {
        return Value::Object(Map::new());
    };
    let want = column.map(compact).filter(|c| !c.is_empty());
    Value::Object(
        map.iter()
            .filter(|(k, _)| want.as_ref().is_none_or(|w| compact(k).contains(w.as_str())))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    )
}

fn paras_of(all: &[Node]) -> Vec<AnnexPara> {
    all.iter()
        .filter(|n| matches!(n.node_type.as_str(), "rule_para" | "note") && n.text != "비고")
        .map(|n| AnnexPara {
            node_id: n.id.clone(),
            path: n.path.clone(),
            node_type: n.node_type.clone(),
            label: n.label.clone(),
            text: n.text.clone(),
        })
        .collect()
}

async fn rules(pool: &PgPool, row_id: i64) -> Result<Vec<AnnexRuleItem>, RetrievalError> {
    let rows = sqlx::query(
        "SELECT rule_id, node_id, kind, params, reviewed FROM annex_rule WHERE annex_row_id = $1 ORDER BY rule_id",
    )
    .bind(row_id)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(AnnexRuleItem {
            rule_id: r.try_get(0)?,
            node_id: r.try_get(1)?,
            kind: r.try_get(2)?,
            params: r.try_get(3)?,
            reviewed: r.try_get(4)?,
        });
    }
    Ok(out)
}

async fn failed_checks(pool: &PgPool, row_id: i64) -> Result<Vec<String>, RetrievalError> {
    Ok(sqlx::query_scalar(
        "SELECT check_name || COALESCE(' (' || NULLIF(detail, '') || ')', '') FROM annex_parse_check \
         WHERE annex_row_id = $1 AND NOT passed ORDER BY check_name",
    )
    .bind(row_id)
    .fetch_all(pool)
    .await?)
}

async fn related(
    pool: &PgPool,
    annex_key: &str,
    as_of: Option<NaiveDate>,
) -> Result<Vec<RelatedArticle>, RetrievalError> {
    let sql = format!(
        "SELECT l.name, av.article_no, r.ref_type, r.ref_text, av.article_version_id FROM article_annex_ref r \
         JOIN article_version av ON av.article_version_id = r.from_unit_id AND upper(av.sys_period) IS NULL \
         JOIN law l ON l.law_id = av.law_id \
         WHERE r.to_annex_key = $2 AND {VALID_AV} \
         ORDER BY l.name, av.article_key, r.ref_type"
    );
    let rows = sqlx::query(AssertSqlSafe(sql))
        .bind(as_of)
        .bind(annex_key)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(RelatedArticle {
            law_name: r.try_get(0)?,
            article_no: r.try_get(1)?,
            ref_type: r.try_get(2)?,
            ref_text: r.try_get(3)?,
            version_id: r.try_get(4)?,
        });
    }
    Ok(out)
}

fn common_notes(v: &AnnexVersion, has_nodes: bool) -> Vec<String> {
    let mut notes = Vec::new();
    if v.annex_type == FORM {
        notes.push("별지서식은 표 복원 대상이 아님 — 원문 글만 있음".to_string());
    } else if !has_nodes {
        notes.push("이 판본은 표 복원 결과가 없음 — 원문 글만 있음".to_string());
    }
    if v.review_status != "reviewed" {
        notes.push(format!(
            "검수 전 판본(review_status={}) — 금액·기준은 원문 확인 필요",
            v.review_status
        ));
    }
    notes
}

pub async fn annex_lookup(pool: &PgPool, q: &AnnexLookup) -> Result<AnnexTable, RetrievalError> {
    let name = AnnexName::parse(&q.annex).ok_or(RetrievalError::Argument("annex"))?;
    if q.law.trim().is_empty() {
        return Err(RetrievalError::Argument("law"));
    }
    let limit = match q.limit {
        0 => LOOKUP_DEFAULT,
        n => n.min(LOOKUP_MAX),
    };
    let (law_id, _, _) = resolve_law(pool, &q.law)
        .await?
        .ok_or(RetrievalError::NotFound("annex"))?;
    let key = name.key(&law_id);
    let loaded = load(pool, q.as_of, Some(&key), None)
        .await?
        .ok_or(RetrievalError::NotFound("annex"))?;
    let all = nodes(pool, loaded.row_id).await?;
    let words: Vec<String> = q
        .words
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(compact)
        .filter(|w| !w.is_empty())
        .collect();
    let matched: Vec<&Node> = all
        .iter()
        .filter(|n| n.cells.is_some() || n.node_type == "row_item")
        .filter(|n| q.row.as_deref().is_none_or(|r| row_matches(n, r)))
        .filter(|n| words_match(n, &words))
        .collect();
    let rows_matched = matched.len();
    let rows: Vec<AnnexRow> = matched
        .into_iter()
        .take(limit)
        .map(|n| AnnexRow {
            node_id: n.id.clone(),
            path: n.path.clone(),
            node_type: n.node_type.clone(),
            label: n.label.clone(),
            table: table_index(&n.path),
            text: n.text.clone(),
            cells: n.cells.clone().unwrap_or(Value::Null),
            amounts: keep_columns(&n.amounts, q.column.as_deref()),
            basis_unit_ids: n.basis.clone(),
        })
        .collect();
    let mut paras = paras_of(&all);
    let mut notes = common_notes(&loaded.version, !all.is_empty());
    if rows_matched > rows.len() {
        notes.push(format!("조건에 맞는 행 {rows_matched}개 가운데 {}개만 실음", rows.len()));
    }
    if paras.len() > PARAS_MAX {
        notes.push(format!("일반기준·비고 문단 {}개 가운데 {PARAS_MAX}개만 실음", paras.len()));
        paras.truncate(PARAS_MAX);
    }
    let rules = rules(pool, loaded.row_id).await?;
    if rules.is_empty() {
        notes.push("일반기준을 규칙으로 옮긴 것이 없음 — 가중·감경은 paras의 원문으로 판단".to_string());
    }
    Ok(AnnexTable {
        related: related(pool, &loaded.version.annex_key, q.as_of).await?,
        failed_checks: failed_checks(pool, loaded.row_id).await?,
        raw_text: all.is_empty().then(|| loaded.raw_text.clone()),
        columns: loaded.columns,
        annex: loaded.version,
        rows,
        rows_matched,
        paras,
        rules,
        notes,
    })
}

fn whole(v: Option<f64>) -> Option<i64> {
    let v = v?;
    (v.fract() == 0.0 && v > 0.0 && v < 1e9).then_some(v as i64)
}

pub fn formula_amount(a: &Value, elapsed: i64) -> Result<(i64, String), String> {
    let base = a.get("base_krw").and_then(Value::as_i64).ok_or("산식 기본액을 읽지 못함")?;
    let inc = a.get("increment_krw").and_then(Value::as_i64).ok_or("산식 증가액을 읽지 못함")?;
    let start = whole(a.get("start").and_then(Value::as_f64)).ok_or("산식 시작 시점을 읽지 못함")?;
    let step = whole(a.get("step").and_then(Value::as_f64)).ok_or("산식 단위 기간을 읽지 못함")?;
    let unit = a.get("start_unit").and_then(Value::as_str).unwrap_or("");
    if elapsed < start {
        return Err(format!("경과 {elapsed}{unit}은 산식 시작({start}{unit}째) 전 — 다른 구간의 행을 확인"));
    }
    let times = (elapsed - start) / step + 1;
    let raw = base + inc * times;
    let cap = a.get("cap_krw").and_then(Value::as_i64);
    let amount = cap.map_or(raw, |c| raw.min(c));
    let mut work = format!("{base}원 + {inc}원 × {times}회 = {raw}원");
    if let Some(c) = cap.filter(|c| raw > *c) {
        work.push_str(&format!(" → 최고 {c}원"));
    }
    Ok((amount, work))
}

fn compute_one(column: &str, item: Option<usize>, cell_text: &str, a: &Value, elapsed: Option<i64>) -> Computed {
    let kind = a.get("kind").and_then(Value::as_str).unwrap_or("text").to_string();
    let source = a.get("source").and_then(Value::as_str).map(str::to_string);
    let mut out = Computed {
        column: column.to_string(),
        item,
        cell_text: cell_text.to_string(),
        kind: kind.clone(),
        krw: None,
        work: None,
        source,
        note: None,
    };
    match kind.as_str() {
        "fixed" => out.krw = a.get("krw").and_then(Value::as_i64),
        "formula" => match elapsed {
            None => out.note = Some("산식 칸 — 경과 기간(elapsed)을 주어야 계산함".to_string()),
            Some(e) => match formula_amount(a, e) {
                Ok((krw, work)) => {
                    out.krw = Some(krw);
                    out.work = Some(work);
                }
                Err(why) => out.note = Some(why),
            },
        },
        _ => out.note = Some("금액으로 읽지 못한 칸 — 원문 그대로".to_string()),
    }
    out
}

pub async fn annex_compute(pool: &PgPool, q: &AnnexCompute) -> Result<Computation, RetrievalError> {
    let node_id = q.node_id.trim();
    if node_id.is_empty() {
        return Err(RetrievalError::Argument("node_id"));
    }
    let row_id: i64 = sqlx::query_scalar("SELECT annex_row_id FROM annex_node WHERE node_id = $1")
        .bind(node_id)
        .fetch_optional(pool)
        .await?
        .ok_or(RetrievalError::NotFound("annex row"))?;
    let loaded = load(pool, None, None, Some(row_id))
        .await?
        .ok_or(RetrievalError::NotFound("annex row"))?;
    let all = nodes(pool, row_id).await?;
    let node = all
        .iter()
        .find(|n| n.id == node_id)
        .cloned()
        .ok_or(RetrievalError::NotFound("annex row"))?;
    let amounts = keep_columns(&node.amounts, q.column.as_deref());
    let order: Vec<String> = table_index(&node.path)
        .and_then(|i| loaded.columns.get(i).cloned())
        .unwrap_or_default();
    let mut keys: Vec<String> = amounts.as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default();
    keys.sort_by_key(|k| order.iter().position(|c| c == k).unwrap_or(usize::MAX));
    let mut results = Vec::new();
    for key in keys {
        let a = &amounts[&key];
        let cell_text = node
            .cells
            .as_ref()
            .and_then(|c| c.get(&key))
            .and_then(|c| c.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if a.get("kind").and_then(Value::as_str) == Some("list") {
            for (i, it) in a.get("items").and_then(Value::as_array).cloned().unwrap_or_default().iter().enumerate() {
                results.push(compute_one(&key, Some(i + 1), &cell_text, it, q.elapsed));
            }
        } else {
            results.push(compute_one(&key, None, &cell_text, a, q.elapsed));
        }
    }
    let mut notes = common_notes(&loaded.version, true);
    if node.amounts.is_none() {
        notes.push("이 행에는 금액 칸이 없음".to_string());
    } else if results.is_empty() {
        notes.push("column에 맞는 금액 칸이 없음".to_string());
    }
    if results.iter().any(|r| r.item.is_some()) {
        notes.push("한 칸에 값이 여럿 — 분기 조건(차수·차종 등)을 사실에서 확인".to_string());
    }
    let rules = rules(pool, row_id).await?;
    notes.push("일반기준(가중·감경·상한)은 적용하지 않은 값 — paras를 함께 제시".to_string());
    Ok(Computation {
        annex: loaded.version,
        node_id: node.id.clone(),
        path: node.path.clone(),
        label: node.label.clone(),
        row_text: node.text.clone(),
        basis_unit_ids: node.basis.clone(),
        results,
        paras: paras_of(&all),
        rules,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn annex_names_cover_branch_numbers_and_form_aliases() {
        let six = AnnexName::parse("[별표 6]");
        assert_eq!(six.as_ref().map(|n| n.key("003395")), Some("003395|별표|6|0".to_string()));
        assert_eq!(six.map(|n| n.label()), Some("별표 6".to_string()));
        let branch = AnnexName::parse("별표 6의 2");
        assert_eq!(branch.as_ref().map(|n| (n.no, n.sub)), Some((6, 2)));
        assert_eq!(branch.map(|n| n.label()), Some("별표 6의2".to_string()));
        assert_eq!(AnnexName::parse("별지 3").map(|n| n.annex_type), Some(FORM.to_string()));
        assert_eq!(AnnexName::parse("별표"), None);
        assert_eq!(AnnexName::parse("별표 6 과태료"), None);
        assert_eq!(AnnexName::parse("제6조"), None);
    }

    fn prototype_formula() -> Value {
        json!({"kind": "formula", "base_krw": 20000, "start": 91.0, "start_unit": "일",
               "step": 3.0, "step_unit": "일", "increment_krw": 10000, "cap_krw": null,
               "source": "2만원에 91일째부터 3일 초과 시마다 1만원을 더한 금액"})
    }

    #[test]
    fn formula_matches_the_prototype_values_and_boundary() {
        let f = prototype_formula();
        let at = |d| formula_amount(&f, d).map(|(v, _)| v);
        assert_eq!(at(91), Ok(30_000));
        assert_eq!(at(94), Ok(40_000));
        assert_eq!(at(120), Ok(120_000));
        assert_eq!(at(174), Ok(300_000));
        assert!(at(90).is_err());
        let (_, work) = formula_amount(&f, 120).unwrap_or_default();
        assert_eq!(work, "20000원 + 10000원 × 10회 = 120000원");
    }

    #[test]
    fn formula_cap_and_unreadable_parts() {
        let mut f = prototype_formula();
        f["cap_krw"] = json!(50_000);
        assert_eq!(formula_amount(&f, 174).map(|(v, _)| v), Ok(50_000));
        f["step"] = json!(2.5);
        assert!(formula_amount(&f, 120).is_err());
    }

    #[test]
    fn compute_one_reads_fixed_text_and_formula_without_elapsed() {
        let fixed = compute_one("금액/1차", None, "9만원", &json!({"kind": "fixed", "krw": 90000}), None);
        assert_eq!(fixed.krw, Some(90_000));
        let text = compute_one("금액", None, "영업정지", &json!({"kind": "text", "source": "영업정지"}), None);
        assert_eq!(text.krw, None);
        assert!(text.note.is_some());
        let f = compute_one("금액", None, "", &prototype_formula(), None);
        assert_eq!(f.krw, None);
        let f = compute_one("금액", None, "", &prototype_formula(), Some(120));
        assert_eq!(f.krw, Some(120_000));
    }

    fn node(id: &str, parent: Option<&str>, ord: i32, path: &str) -> Node {
        Node {
            id: id.into(),
            parent: parent.map(str::to_string),
            path: path.into(),
            node_type: "row".into(),
            ordinal: ord,
            label: None,
            text: String::new(),
            cells: None,
            amounts: None,
            basis: Vec::new(),
        }
    }

    #[test]
    fn document_order_follows_parent_ordinals_not_path_text() {
        let nodes = vec![
            node("r10", Some("t"), 10, "a1.s.t1.r10"),
            node("s", Some("root"), 1, "a1.s"),
            node("r2", Some("t"), 2, "a1.s.t1.r2"),
            node("root", None, 0, "a1.본문"),
            node("t", Some("s"), 1, "a1.s.t1"),
        ];
        let ids: Vec<String> = in_document_order(nodes).into_iter().map(|n| n.id).collect();
        assert_eq!(ids, vec!["root", "s", "t", "r2", "r10"]);
    }

    #[test]
    fn rows_are_found_by_label_path_suffix_or_id_and_columns_filter_amounts() {
        let mut n = node("v#a2.개별기준.t1.가", None, 1, "a2.개별기준.t1.가");
        n.label = Some("가".into());
        assert!(row_matches(&n, "가"));
        assert!(row_matches(&n, "가."));
        assert!(row_matches(&n, "t1.가"));
        assert!(!row_matches(&n, "나"));
        assert_eq!(table_index(&n.path), Some(0));
        let amounts = Some(json!({"과태료/1차 위반": {"kind": "fixed", "krw": 1}, "과태료/2차 이상 위반": {"kind": "fixed", "krw": 2}}));
        let kept = keep_columns(&amounts, Some("2차"));
        assert_eq!(kept.as_object().map(|m| m.len()), Some(1));
        assert_eq!(keep_columns(&amounts, None).as_object().map(|m| m.len()), Some(2));
    }
}
