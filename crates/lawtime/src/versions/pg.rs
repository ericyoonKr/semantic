use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use chrono::{DateTime, NaiveDate, Utc};
use regex::Regex;
use serde::Serialize;
use sqlx::PgPool;

use super::{EndReason, PromulNo, UnitKey, UnitKind, Version, VersionStore};
use crate::Error;
use crate::citation::{CiteFlag, CiteStatus, CitationResolution, MistakeReason};
use crate::period::Period;
use crate::resolve::{Flag, Resolution};

const LAW_SQL: &str = "SELECT law_id, name, law_type, kind FROM law \
     WHERE law_id = $1 OR regexp_replace(name, '\\s', '', 'g') = $2 ORDER BY law_id";

const PROMUL_SQL: &str = "SELECT promulgation_no, promulgation_date, enforcement_date, coalesce(revision_type, '') \
     FROM law_version WHERE law_id = $1 AND kind = 'law' AND promulgation_no IS NOT NULL";

const ROWS_SQL: &str = "SELECT article_no, article_key, promul_no, promul_date, lower(valid_period), \
     upper(valid_period), isempty(valid_period), end_reason, body, date_status, api_article_date \
     FROM article_version WHERE law_id = $1 AND article_no = ANY($2) \
     AND CASE WHEN $3::timestamptz IS NULL THEN upper(sys_period) IS NULL ELSE sys_period @> $3::timestamptz END \
     ORDER BY article_no, lower(valid_period)";

const ALIAS_SQL: &str = "SELECT DISTINCT a.moved_from, v.article_no FROM article_key_alias a \
     JOIN article_version v ON v.law_id = a.law_id AND v.article_key = a.article_key \
     WHERE a.law_id = $1 AND a.moved_from = ANY($2)";

const RULE_SQL: &str = "SELECT rule_id, promul_no, addendum_article, addendum_title, rule_type, \
     retired_by_full_revision, kept_flag, reviewed_by FROM addendum_rule WHERE law_id = $1 \
     AND (rule_type IS NULL OR rule_type IN ('APPLICATION', 'TRANSITION_KEEP_OLD', 'SUNSET'))";

const REVIEW_SQL: &str = "SELECT article_key, promul_no, reason FROM effective_date_review \
     WHERE law_id = $1 AND status = 'open' AND article_key = ANY($2)";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LawRef {
    pub law_id: String,
    pub name: String,
    pub law_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum LawLookup {
    One(LawRef),
    NotFound,
    Several(Vec<LawRef>),
    NotStatute(Vec<LawRef>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Promulgation {
    pub no: PromulNo,
    pub date: Option<NaiveDate>,
    pub effective: Option<NaiveDate>,
    pub full_revision: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredRule {
    pub rule_id: String,
    pub promul_no: PromulNo,
    pub addendum_article: String,
    pub title: Option<String>,
    pub rule_type: Option<String>,
    pub retired: bool,
    pub kept: bool,
    pub reviewed_by: Option<String>,
}

#[derive(Debug, Clone)]
struct Row {
    version: Version,
    article_key: String,
    date_status: String,
    api_date: Option<NaiveDate>,
}

#[derive(Debug, Clone)]
struct LawData {
    law: LawRef,
    promuls: Vec<Promulgation>,
    rules: Vec<StoredRule>,
    reviews: Vec<(String, u32, String)>,
}

#[derive(Debug, Default)]
pub struct PgStore {
    laws: BTreeMap<String, LawData>,
    rows: BTreeMap<UnitKey, Vec<Row>>,
    undetermined: BTreeMap<UnitKey, Vec<PromulNo>>,
    aliases: BTreeMap<UnitKey, Vec<UnitKey>>,
    known_at: Option<DateTime<Utc>>,
}

type RowTuple = (
    String,
    String,
    String,
    Option<NaiveDate>,
    Option<NaiveDate>,
    Option<NaiveDate>,
    bool,
    Option<String>,
    String,
    String,
    Option<NaiveDate>,
);

type RuleTuple = (String, String, String, Option<String>, Option<String>, bool, bool, Option<String>);

pub fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn article_re() -> Result<&'static Regex, Error> {
    static CELL: OnceLock<Result<Regex, String>> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"^\s*제\s*(\d+)\s*조(?:\s*의\s*(\d+))?").map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| Error::Regex(e.clone()))
}

pub fn article_of(cited: &str) -> Result<Option<String>, Error> {
    let Some(c) = article_re()?.captures(cited) else { return Ok(None) };
    let Some(no) = c.get(1).and_then(|m| m.as_str().parse::<u32>().ok()) else { return Ok(None) };
    let branch = c.get(2).and_then(|m| m.as_str().parse::<u32>().ok()).filter(|b| *b > 0);
    Ok(Some(match branch {
        Some(b) => format!("제{no}조의{b}"),
        None => format!("제{no}조"),
    }))
}

fn fits(stored: &PromulNo, asked: &PromulNo) -> bool {
    stored.number == asked.number
        && (stored.issuer == asked.issuer || (stored.issuer == "부령" && asked.issuer.ends_with("부령")))
}

fn end_reason(s: Option<&str>) -> Result<Option<EndReason>, Error> {
    Ok(match s {
        None => None,
        Some("amended") => Some(EndReason::Amended),
        Some("repealed") => Some(EndReason::Repealed),
        Some("expired") => Some(EndReason::Expired),
        Some("unconstitutional") => Some(EndReason::Unconstitutional),
        Some(other) => return Err(Error::Data(format!("모르는 종료 사유: {other}"))),
    })
}

fn number(s: &str) -> Result<u32, Error> {
    s.trim().parse().map_err(|_| Error::Data(format!("공포번호가 숫자가 아님: {s}")))
}

fn same_or_none<T: PartialEq + Copy>(a: Option<T>, b: Option<T>) -> Option<T> {
    if a == b { a } else { None }
}

pub async fn find_law(pool: &PgPool, name: &str) -> Result<LawLookup, Error> {
    let name = name.trim();
    let rows: Vec<(String, String, String, String)> =
        sqlx::query_as(LAW_SQL).bind(name).bind(compact(name)).fetch_all(pool).await?;
    let to_ref = |r: &(String, String, String, String)| LawRef {
        law_id: r.0.clone(),
        name: r.1.clone(),
        law_type: r.2.clone(),
    };
    if let Some(r) = rows.iter().find(|r| r.0 == name) {
        return Ok(if r.3 == "law" { LawLookup::One(to_ref(r)) } else { LawLookup::NotStatute(vec![to_ref(r)]) });
    }
    let statutes: Vec<LawRef> = rows.iter().filter(|r| r.3 == "law").map(to_ref).collect();
    Ok(match statutes.as_slice() {
        [] if rows.is_empty() => LawLookup::NotFound,
        [] => LawLookup::NotStatute(rows.iter().map(to_ref).collect()),
        [one] => LawLookup::One(one.clone()),
        _ => LawLookup::Several(statutes),
    })
}

async fn promulgations(pool: &PgPool, law: &LawRef) -> Result<Vec<Promulgation>, Error> {
    let raw: Vec<(i32, Option<NaiveDate>, Option<NaiveDate>, String)> =
        sqlx::query_as(PROMUL_SQL).bind(&law.law_id).fetch_all(pool).await?;
    let mut by_no: BTreeMap<u32, Promulgation> = BTreeMap::new();
    for (no, date, effective, revision) in raw {
        let n = u32::try_from(no).map_err(|_| Error::Data(format!("공포번호가 음수: {no}")))?;
        let full = revision.trim() == "전부개정";
        by_no
            .entry(n)
            .and_modify(|p| {
                p.date = same_or_none(p.date, date);
                p.effective = same_or_none(p.effective, effective);
                p.full_revision |= full;
            })
            .or_insert(Promulgation {
                no: PromulNo::new(&law.law_type, n),
                date,
                effective,
                full_revision: full,
            });
    }
    Ok(by_no.into_values().collect())
}

impl PgStore {
    pub async fn load(
        pool: &PgPool,
        laws: &[LawRef],
        units: &[UnitKey],
        known_at: Option<DateTime<Utc>>,
    ) -> Result<Self, Error> {
        let mut store = PgStore { known_at, ..Self::default() };
        for law in laws {
            let cited: Vec<String> = units
                .iter()
                .filter(|u| u.law_id == law.law_id && u.kind == UnitKind::Article)
                .map(|u| u.key.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let aliases: Vec<(String, String)> =
                sqlx::query_as(ALIAS_SQL).bind(&law.law_id).bind(&cited).fetch_all(pool).await?;
            let mut wanted: BTreeSet<String> = cited.iter().cloned().collect();
            for (old, current) in aliases {
                wanted.insert(current.clone());
                store
                    .aliases
                    .entry(UnitKey::article(&law.law_id, &old))
                    .or_default()
                    .push(UnitKey::article(&law.law_id, &current));
            }
            let wanted: Vec<String> = wanted.into_iter().collect();
            let promuls = promulgations(pool, law).await?;
            let raw: Vec<RowTuple> = sqlx::query_as(ROWS_SQL)
                .bind(&law.law_id)
                .bind(&wanted)
                .bind(known_at)
                .fetch_all(pool)
                .await?;
            let mut keys = BTreeSet::new();
            for r in raw {
                keys.insert(r.1.clone());
                store.add_row(law, &promuls, r)?;
            }
            let raw_rules: Vec<RuleTuple> = sqlx::query_as(RULE_SQL).bind(&law.law_id).fetch_all(pool).await?;
            let mut rules = Vec::new();
            for (rule_id, no, article, title, rule_type, retired, kept, reviewed_by) in raw_rules {
                let Ok(n) = number(&no) else { continue };
                rules.push(StoredRule {
                    rule_id,
                    promul_no: PromulNo::new(&law.law_type, n),
                    addendum_article: article,
                    title,
                    rule_type,
                    retired,
                    kept,
                    reviewed_by,
                });
            }
            let keys: Vec<String> = keys.into_iter().collect();
            let raw_reviews: Vec<(String, String, String)> =
                sqlx::query_as(REVIEW_SQL).bind(&law.law_id).bind(&keys).fetch_all(pool).await?;
            let reviews = raw_reviews
                .into_iter()
                .filter_map(|(k, no, reason)| number(&no).ok().map(|n| (k, n, reason)))
                .collect();
            store.laws.insert(law.law_id.clone(), LawData { law: law.clone(), promuls, rules, reviews });
        }
        Ok(store)
    }

    fn add_row(&mut self, law: &LawRef, promuls: &[Promulgation], r: RowTuple) -> Result<(), Error> {
        let (article_no, article_key, no, promul_date, lower, upper, empty, end, body, date_status, api_date) = r;
        let unit = UnitKey::article(&law.law_id, &article_no);
        let promul_no = PromulNo::new(&law.law_type, number(&no)?);
        if empty {
            self.undetermined.entry(unit).or_default().push(promul_no);
            return Ok(());
        }
        let from = lower.ok_or_else(|| Error::Data(format!("{unit} 시작일 없음")))?;
        let promul_date = promul_date
            .or_else(|| promuls.iter().find(|p| fits(&p.no, &promul_no)).and_then(|p| p.date))
            .ok_or_else(|| Error::Data(format!("{unit} {promul_no} 공포일 없음")))?;
        let version = Version {
            unit: unit.clone(),
            promul_no,
            promul_date,
            valid_period: Period::new(from, upper)?,
            end_reason: end_reason(end.as_deref())?,
            body,
        };
        let rows = self.rows.entry(unit).or_default();
        if rows.iter().any(|x| x.version.valid_period.overlaps(&version.valid_period)) {
            return Err(Error::Overlap(format!("{} {}", version.unit, from)));
        }
        rows.push(Row { version, article_key, date_status, api_date });
        Ok(())
    }

    pub fn known_at(&self) -> Option<DateTime<Utc>> {
        self.known_at
    }

    pub fn law(&self, law_id: &str) -> Option<&LawRef> {
        self.laws.get(law_id).map(|d| &d.law)
    }

    pub fn first_effective(&self, law_id: &str) -> Option<NaiveDate> {
        self.laws.get(law_id)?.promuls.iter().filter_map(|p| p.effective).min()
    }

    fn rows_of(&self, unit: &UnitKey) -> &[Row] {
        self.rows.get(unit).map_or(&[], Vec::as_slice)
    }

    fn promuls_of(&self, law_id: &str) -> &[Promulgation] {
        self.laws.get(law_id).map_or(&[], |d| d.promuls.as_slice())
    }

    fn version_notes(&self, v: &Version) -> Vec<String> {
        let mut out = Vec::new();
        let Some(row) = self.rows_of(&v.unit).iter().find(|r| r.version.same_row(v)) else { return out };
        if row.date_status == "paragraph_varies" {
            out.push(format!(
                "{} {}: 항마다 시행일이 다른 조문 — 항에 따라 판본 경계가 다를 수 있음",
                v.unit, v.valid_period.from
            ));
        }
        if let Some(api) = row.api_date.filter(|a| *a != v.valid_period.from) {
            out.push(format!(
                "{} {}: 조문시행일자문자열 기준 시작일과 API 조문시행일자 {api}가 다름",
                v.unit, v.valid_period.from
            ));
        }
        if let Some(law) = self.laws.get(&v.unit.law_id) {
            for (_, _, reason) in law
                .reviews
                .iter()
                .filter(|(k, n, _)| *k == row.article_key && *n == v.promul_no.number)
            {
                out.push(format!("{} {}: 시행일 검토 대기({reason})", v.unit, v.promul_no));
            }
        }
        out
    }

    fn full_revision_after(&self, law_id: &str, rule: &StoredRule) -> Option<&Promulgation> {
        let promuls = self.promuls_of(law_id);
        let since = promuls.iter().find(|p| fits(&p.no, &rule.promul_no)).and_then(|p| p.date)?;
        promuls
            .iter()
            .filter(|p| p.full_revision && p.date.is_some_and(|d| d > since))
            .min_by_key(|p| p.date)
    }

    fn rule_flag(&self, unit: &UnitKey, rule: &StoredRule) -> Flag {
        let head = format!(
            "{} {}{}",
            rule.promul_no,
            rule.addendum_article,
            rule.title.as_deref().map(|t| format!("({t})")).unwrap_or_default()
        );
        match rule.rule_type.as_deref() {
            Some("APPLICATION" | "TRANSITION_KEEP_OLD") if rule.retired && !rule.kept => {
                match self.full_revision_after(&unit.law_id, rule) {
                    Some(p) => match p.effective {
                        Some(effective) => Flag::FullRevisionRetired {
                            rule_id: rule.rule_id.clone(),
                            by: p.no.clone(),
                            effective,
                        },
                        None => Flag::AddendumUndetermined {
                            rule_id: rule.rule_id.clone(),
                            note: format!("{head} — 전부개정으로 실효 표시가 있으나 전부개정 시행일을 자료에서 찾지 못함"),
                        },
                    },
                    None => Flag::AddendumUndetermined {
                        rule_id: rule.rule_id.clone(),
                        note: format!("{head} — 전부개정으로 실효 표시가 있으나 전부개정 공포를 자료에서 찾지 못함"),
                    },
                }
            }
            Some(kind @ ("APPLICATION" | "TRANSITION_KEEP_OLD")) => Flag::AddendumUndetermined {
                rule_id: rule.rule_id.clone(),
                note: format!("{head} [{kind}] — 부칙 규칙의 대상 조문·사건 술어가 구조화되지 않아 평가하지 못함"),
            },
            Some("SUNSET") => Flag::Review {
                rule_id: rule.rule_id.clone(),
                note: format!("{head} — 유효기간 부칙: 한시법 실효 여부 확인"),
            },
            _ => Flag::Review {
                rule_id: rule.rule_id.clone(),
                note: format!("{head} — 유형을 분류하지 못한 부칙 조항"),
            },
        }
    }

    pub fn check_resolution(&self, unit: &UnitKey, res: Resolution, as_of: NaiveDate) -> Resolution {
        let Some(date) = res.ref_date() else { return res };
        let lo = date.lower();
        let mut flags = Vec::new();
        match self.first_effective(&unit.law_id) {
            Some(first) if lo < first => flags.push(Flag::DataHold {
                unit: unit.clone(),
                note: format!("기준시 {lo}가 법령 DB 판본 자료의 시작({first}) 앞 — 그 전 판본은 자료에 없음"),
            }),
            None => flags.push(Flag::DataHold {
                unit: unit.clone(),
                note: "이 법령의 공포 자료가 DB에 없음".to_string(),
            }),
            Some(_) => {}
        }
        if self.rows_of(unit).is_empty() && !self.undetermined.contains_key(unit) {
            flags.push(Flag::DataHold { unit: unit.clone(), note: "이 조문의 판본 행이 DB에 없음".to_string() });
        }
        for p in self.undetermined.get(unit).map_or(&[][..], Vec::as_slice) {
            let promulgated = self.promul_date(&unit.law_id, p).ok().flatten();
            if promulgated.is_none_or(|d| d <= as_of) {
                flags.push(Flag::DataHold {
                    unit: unit.clone(),
                    note: format!("시행일이 정해지지 않은 판본이 있음({p}) — 기준시 판본이 바뀔 수 있음"),
                });
            }
        }
        for v in &res.versions {
            for note in self.version_notes(v) {
                flags.push(Flag::DataReview { unit: v.unit.clone(), note });
            }
        }
        let mut relevant: Vec<&PromulNo> = res.versions.iter().map(|v| &v.promul_no).collect();
        relevant.extend(
            self.rows_of(unit)
                .iter()
                .map(|r| &r.version)
                .filter(|v| v.valid_period.from > lo && v.valid_period.from <= as_of)
                .map(|v| &v.promul_no),
        );
        if let Some(law) = self.laws.get(&unit.law_id) {
            for rule in law.rules.iter().filter(|r| relevant.iter().any(|p| fits(p, &r.promul_no))) {
                flags.push(self.rule_flag(unit, rule));
            }
        }
        res.with_flags(flags)
    }

    pub fn check_citation(&self, mut cite: CitationResolution, cited_dates: &[NaiveDate]) -> CitationResolution {
        if let Some(v) = cite.version.clone() {
            for note in self.version_notes(&v) {
                cite.flags.push(CiteFlag::Data(note));
            }
        }
        let why = match &cite.status {
            CiteStatus::MistakeCandidate(w) => w.clone(),
            _ => return cite,
        };
        let first = self.first_effective(&cite.unit.law_id);
        let before = |d: NaiveDate| first.is_none_or(|f| d < f);
        let no_rows = self.rows_of(&cite.unit).is_empty();
        let outside = match &why {
            MistakeReason::UnknownPromulgation(_) | MistakeReason::NoVersionForPromul(_) => {
                no_rows || cited_dates.iter().any(|d| before(*d))
            }
            MistakeReason::NoEffectiveDate(no) => {
                no_rows
                    || cited_dates.iter().any(|d| before(*d))
                    || self.undetermined.get(&cite.unit).is_some_and(|ps| ps.iter().any(|p| fits(p, no)))
            }
            MistakeReason::NoVersionAt(t) => no_rows || before(*t),
            MistakeReason::PromulDateMismatch { .. } | MistakeReason::InvertedBounds { .. } => false,
        };
        if outside {
            cite.status = CiteStatus::OutsideData { first, why };
        }
        cite
    }
}

impl VersionStore for PgStore {
    fn at(&self, unit: &UnitKey, date: NaiveDate) -> Result<Option<Version>, Error> {
        let hits: Vec<&Version> =
            self.rows_of(unit).iter().map(|r| &r.version).filter(|v| v.valid_period.contains(date)).collect();
        match hits.as_slice() {
            [] => Ok(None),
            [v] => Ok(Some((*v).clone())),
            _ => Err(Error::Ambiguous(format!("{unit} {date}"))),
        }
    }

    fn between(&self, unit: &UnitKey, from: NaiveDate, to: Option<NaiveDate>) -> Result<Vec<Version>, Error> {
        Ok(self
            .rows_of(unit)
            .iter()
            .map(|r| &r.version)
            .filter(|v| v.valid_period.meets_closed(from, to))
            .cloned()
            .collect())
    }

    fn by_promul(&self, unit: &UnitKey, promul_no: &PromulNo) -> Result<Option<Version>, Error> {
        let hits: Vec<&Version> =
            self.rows_of(unit).iter().map(|r| &r.version).filter(|v| fits(&v.promul_no, promul_no)).collect();
        match hits.as_slice() {
            [] => Ok(None),
            [v] => Ok(Some((*v).clone())),
            _ => Err(Error::Ambiguous(format!("{unit} {promul_no}"))),
        }
    }

    fn effective_date(&self, unit: &UnitKey, promul_no: &PromulNo) -> Result<Option<NaiveDate>, Error> {
        if self.undetermined.get(unit).is_some_and(|ps| ps.iter().any(|p| fits(p, promul_no))) {
            return Ok(None);
        }
        if let Some(v) = self.by_promul(unit, promul_no)? {
            return Ok(Some(v.valid_period.from));
        }
        Ok(self
            .promuls_of(&unit.law_id)
            .iter()
            .find(|p| fits(&p.no, promul_no))
            .and_then(|p| p.effective))
    }

    fn promul_date(&self, law_id: &str, promul_no: &PromulNo) -> Result<Option<NaiveDate>, Error> {
        if let Some(p) = self.promuls_of(law_id).iter().find(|p| fits(&p.no, promul_no)) {
            return Ok(p.date);
        }
        Ok(self
            .rows
            .iter()
            .filter(|(u, _)| u.law_id == law_id)
            .flat_map(|(_, rows)| rows.iter())
            .map(|r| &r.version)
            .find(|v| fits(&v.promul_no, promul_no))
            .map(|v| v.promul_date))
    }

    fn current_key(&self, unit: &UnitKey) -> Result<Option<UnitKey>, Error> {
        if !self.rows_of(unit).is_empty() || self.undetermined.contains_key(unit) {
            return Ok(None);
        }
        Ok(match self.aliases.get(unit).map(Vec::as_slice) {
            Some([one]) => Some(one.clone()),
            _ => None,
        })
    }
}
