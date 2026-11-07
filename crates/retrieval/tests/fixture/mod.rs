#![allow(dead_code)]

use std::error::Error;
use std::fmt::Display;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::postgres::{PgConnection, PgPoolOptions};
use sqlx::{AssertSqlSafe, Connection, PgPool};

pub type Failure = Box<dyn Error + Send + Sync>;
pub type Checked = Result<(), Failure>;

pub const URL_VAR: &str = "RETRIEVAL_TEST_DATABASE_URL";
const LOCK_KEY: i64 = 7_203_311_419;

pub struct Fixture {
    pub schema: String,
    pub url: String,
    admin: PgPool,
    lock: PgConnection,
}

pub fn check(ok: bool, what: impl Display) -> Checked {
    if ok { Ok(()) } else { Err(what.to_string().into()) }
}

const TABLES: &[&str] = &[
    "CREATE TABLE law (law_id text PRIMARY KEY, kind text NOT NULL, name text NOT NULL, law_type text NOT NULL)",
    "CREATE TABLE law_version (kind text NOT NULL, lsi_seq bigint NOT NULL, law_id text NOT NULL REFERENCES law, \
     enforcement_date date, status text NOT NULL, PRIMARY KEY (kind, lsi_seq))",
    "CREATE TABLE law_unit_lineage (lineage_key text PRIMARY KEY, law_id text NOT NULL REFERENCES law, unit_path text NOT NULL)",
    "CREATE TABLE law_unit (unit_id bigint PRIMARY KEY, kind text NOT NULL, lsi_seq bigint NOT NULL, unit_key text NOT NULL, \
     level text NOT NULL, title text, body text NOT NULL, deleted boolean NOT NULL DEFAULT false, enforcement_date date, \
     content_status text NOT NULL DEFAULT 'text', source_urls jsonb NOT NULL DEFAULT '[]', \
     lineage_key text NOT NULL REFERENCES law_unit_lineage, FOREIGN KEY (kind, lsi_seq) REFERENCES law_version)",
    "CREATE TABLE court (court_id smallint PRIMARY KEY, court_key text NOT NULL, name text NOT NULL, level text NOT NULL)",
    "CREATE TABLE decision (decision_id bigint PRIMARY KEY, court_id smallint NOT NULL REFERENCES court, case_no text NOT NULL, \
     case_no_display text NOT NULL, decided_on date NOT NULL, field text NOT NULL, is_bundle boolean NOT NULL DEFAULT false)",
    "CREATE TABLE section_role (role text PRIMARY KEY, label text NOT NULL)",
    "CREATE TABLE decision_section (section_id bigint PRIMARY KEY, decision_id bigint NOT NULL REFERENCES decision, \
     ord integer NOT NULL, role text NOT NULL REFERENCES section_role, heading text, body text NOT NULL)",
    "CREATE TABLE decision_chunk (chunk_id bigint PRIMARY KEY, section_id bigint NOT NULL REFERENCES decision_section, \
     ord integer NOT NULL, char_start integer NOT NULL, char_end integer NOT NULL)",
    "CREATE TABLE extract_run (run_id integer PRIMARY KEY, extractor text NOT NULL, version text NOT NULL, is_active boolean NOT NULL)",
    "CREATE TABLE decision_citation (citation_id bigint PRIMARY KEY, run_id integer NOT NULL REFERENCES extract_run, \
     decision_id bigint NOT NULL REFERENCES decision, section_id bigint NOT NULL REFERENCES decision_section, \
     char_start integer NOT NULL, char_end integer NOT NULL, raw text NOT NULL, \
     lineage_key text REFERENCES law_unit_lineage, unit_id bigint REFERENCES law_unit, relation text NOT NULL)",
];

pub const OLD_SEQ: i64 = 100;
pub const CURRENT_SEQ: i64 = 200;
pub const DECREE_SEQ: i64 = 300;
pub const SCHEDULED_SEQ: i64 = 400;

pub const ART2_OLD: &str = "국가는 공무원이 직무를 집행하면서 고의로 타인에게 손해를 입힌 때에는 그 손해를 배상하여야 한다.";
pub const ART2_NOW: &str = "국가나 지방자치단체는 공무원이 직무를 집행하면서 고의 또는 과실로 법령을 위반하여 타인에게 손해를 입힌 때에는 그 손해를 배상하여야 한다.";
pub const ART3_NOW: &str = "배상금은 손해액을 기준으로 정한다.";
pub const DECREE_ART3: &str = "배상심의회는 공무원의 과실 여부를 심의한다.";
pub const SCHEDULED_ART2: &str = "장래 시행 조문: 공무원의 과실로 인한 배상 책임을 다시 정한다.";

pub const REASONS_HEAD: &str = "원심은 사실관계를 확정하였다. 증거를 살펴본다. ";
pub const REASONS_TAIL: &str = "피고 소속 공무원의 과실이 인정되므로 배상 책임이 있다.";
pub const SUMMARY_10: &str = "공무원의 직무상 과실로 인한 국가배상 책임을 인정한 사례.";
pub const SUMMARY_11: &str = "처분의 절차상 하자와 담당 공무원의 과실 여부를 판단하였다.";
pub const BUNDLE_BODY: &str = "묶음 행: 공무원 과실 국가배상 사건 여럿.";

fn rows() -> Vec<String> {
    let head = REASONS_HEAD.chars().count();
    let total = head + REASONS_TAIL.chars().count();
    vec![
        "INSERT INTO law VALUES ('L1','law','국가배상법','법률'), ('L2','law','국가배상법 시행령','대통령령')".into(),
        format!(
            "INSERT INTO law_version VALUES ('law',{OLD_SEQ},'L1','2017-01-01','past'), \
             ('law',{CURRENT_SEQ},'L1','2023-01-01','current'), ('law',{DECREE_SEQ},'L2','2023-01-01','current'), \
             ('law',{SCHEDULED_SEQ},'L1','2030-01-01','scheduled')"
        ),
        "INSERT INTO law_unit_lineage VALUES ('L1#제2조','L1','제2조'), ('L1#제2조제1항','L1','제2조제1항'), \
         ('L1#제3조','L1','제3조'), ('L1#제4조','L1','제4조'), ('L1#별표1','L1','별표 1'), ('L2#제3조','L2','제3조')"
            .into(),
        format!(
            "INSERT INTO law_unit (unit_id, kind, lsi_seq, unit_key, level, title, body, deleted, enforcement_date, \
             content_status, source_urls, lineage_key) VALUES \
             (1,'law',{OLD_SEQ},'제2조','article','배상책임','{ART2_OLD}',false,NULL,'text','[]','L1#제2조'), \
             (2,'law',{CURRENT_SEQ},'제2조','article','배상책임','{ART2_NOW}',false,NULL,'text','[]','L1#제2조'), \
             (4,'law',{CURRENT_SEQ},'제3조','article',NULL,'{ART3_NOW}',false,'2023-07-01','text','[]','L1#제3조'), \
             (5,'law',{CURRENT_SEQ},'제4조','article',NULL,'삭제 공무원 과실 배상',true,NULL,'text','[]','L1#제4조'), \
             (6,'law',{CURRENT_SEQ},'별표 1','annex','배상금 기준표','',false,NULL,'link_only','[\"https://example.invalid/annex-1.pdf\"]','L1#별표1'), \
             (7,'law',{DECREE_SEQ},'제3조','article','심의','{DECREE_ART3}',false,NULL,'text','[]','L2#제3조'), \
             (8,'law',{SCHEDULED_SEQ},'제2조','article','배상책임','{SCHEDULED_ART2}',false,NULL,'text','[]','L1#제2조')"
        ),
        "INSERT INTO court VALUES (1,'supreme','대법원','supreme'), (2,'seoul-high','서울고등법원','high')".into(),
        "INSERT INTO section_role VALUES ('holding_summary','판결요지'), ('reasons','이유')".into(),
        "INSERT INTO decision VALUES (10,1,'2020다1111','2020다1111','2021-03-04','civil',false), \
         (11,2,'2022누2222','2022누2222','2023-05-06','administrative',false), \
         (12,1,'2021다3333','2021다3333 등','2022-01-01','civil',true)"
            .into(),
        format!(
            "INSERT INTO decision_section VALUES \
             (100,10,1,'holding_summary','판결요지','{SUMMARY_10}'), \
             (101,10,2,'reasons','이유','{REASONS_HEAD}{REASONS_TAIL}'), \
             (102,11,1,'holding_summary','판결요지','{SUMMARY_11}'), \
             (103,12,1,'holding_summary',NULL,'{BUNDLE_BODY}')"
        ),
        format!(
            "INSERT INTO decision_chunk VALUES (1000,101,1,0,{head}), (1001,101,2,{head},{total})"
        ),
        "INSERT INTO extract_run VALUES (1,'citation','v1',false), (2,'citation','v2',true), (3,'overrule','v9',true)".into(),
        "INSERT INTO decision_citation VALUES \
         (1,2,10,101,0,5,'국가배상법 제2조','L1#제2조',2,'applies'), \
         (2,2,11,102,0,5,'같은 법 제2조 제1항','L1#제2조제1항',NULL,'interprets'), \
         (3,1,11,102,0,5,'옛 추출 결과','L1#제3조',4,'cites'), \
         (4,2,10,100,0,5,'시행령 제3조','L2#제3조',7,'cites')"
            .into(),
    ]
}

fn unique_schema() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("retrieval_test_{}_{}", std::process::id(), nanos)
}

fn with_search_path(base: &str, path: &str) -> String {
    let sep = if base.contains('?') { '&' } else { '?' };
    format!("{base}{sep}options[search_path]={path}")
}

pub async fn open() -> Result<Option<Fixture>, Failure> {
    let Some(base) = std::env::var(URL_VAR).ok().filter(|v| !v.trim().is_empty()) else {
        eprintln!("{URL_VAR} is not set; database test skipped");
        return Ok(None);
    };
    let mut lock = PgConnection::connect(&base).await?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(LOCK_KEY)
        .execute(&mut lock)
        .await?;
    let schema = unique_schema();
    let admin = PgPoolOptions::new().max_connections(1).connect(&base).await?;
    sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await?;
    let fx = Fixture {
        url: String::new(),
        schema,
        admin,
        lock,
    };
    match fx.fill(&base).await {
        Ok(url) => Ok(Some(Fixture { url, ..fx })),
        Err(e) => {
            fx.close().await?;
            Err(e)
        }
    }
}

impl Fixture {
    async fn fill(&self, base: &str) -> Result<String, Failure> {
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT extnamespace::regnamespace::text FROM pg_extension WHERE extname = 'pg_trgm'",
        )
        .fetch_optional(&self.admin)
        .await?;
        let ext_schema = match existing {
            Some(s) => s,
            None => {
                sqlx::query(AssertSqlSafe(format!(
                    "CREATE EXTENSION pg_trgm WITH SCHEMA {}",
                    self.schema
                )))
                .execute(&self.admin)
                .await?;
                self.schema.clone()
            }
        };
        let path = format!("{},{}", self.schema, ext_schema);
        let mut tx = self.admin.begin().await?;
        sqlx::query(AssertSqlSafe(format!("SET LOCAL search_path TO {path}")))
            .execute(&mut *tx)
            .await?;
        for stmt in TABLES {
            sqlx::query(*stmt).execute(&mut *tx).await?;
        }
        for stmt in rows() {
            sqlx::query(AssertSqlSafe(stmt)).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(with_search_path(base, &path))
    }

    pub async fn close(mut self) -> Checked {
        let dropped = sqlx::query(AssertSqlSafe(format!(
            "DROP SCHEMA IF EXISTS {} CASCADE",
            self.schema
        )))
        .execute(&self.admin)
        .await;
        self.admin.close().await;
        sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(LOCK_KEY)
            .execute(&mut self.lock)
            .await?;
        self.lock.close().await?;
        dropped?;
        Ok(())
    }

    pub async fn schema_exists(base: &str, schema: &str) -> Result<bool, Failure> {
        let mut c = PgConnection::connect(base).await?;
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_namespace WHERE nspname = $1")
            .bind(schema)
            .fetch_one(&mut c)
            .await?;
        c.close().await?;
        Ok(n > 0)
    }
}
