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
    "CREATE TABLE article_version (row_id bigint PRIMARY KEY, article_version_id text NOT NULL, article_key text NOT NULL, \
     law_id text NOT NULL REFERENCES law, article_no text NOT NULL, \
     sys_period tstzrange NOT NULL DEFAULT tstzrange(now(), NULL))",
    "CREATE TABLE court (court_id smallint PRIMARY KEY, court_key text NOT NULL, name text NOT NULL, level text NOT NULL)",
    "CREATE TABLE decision (decision_id bigint PRIMARY KEY, court_id smallint NOT NULL REFERENCES court, case_no text NOT NULL, \
     case_no_display text NOT NULL, decided_on date NOT NULL, field text NOT NULL, is_bundle boolean NOT NULL DEFAULT false, \
     en_banc boolean NOT NULL DEFAULT false, disposition text NOT NULL DEFAULT 'other')",
    "CREATE TABLE section_role (role text PRIMARY KEY, label text NOT NULL)",
    "CREATE TABLE decision_section (section_id bigint PRIMARY KEY, decision_id bigint NOT NULL REFERENCES decision, \
     ord integer NOT NULL, role text NOT NULL REFERENCES section_role, heading text, body text NOT NULL, \
     opinion text CHECK (opinion IN ('majority','dissent','separate','concurring')), \
     direction text CHECK (direction IN ('positive','negative')))",
    "CREATE TABLE decision_chunk (chunk_id bigint PRIMARY KEY, section_id bigint NOT NULL REFERENCES decision_section, \
     ord integer NOT NULL, char_start integer NOT NULL, char_end integer NOT NULL)",
    "CREATE TABLE extract_run (run_id integer PRIMARY KEY, extractor text NOT NULL, version text NOT NULL, is_active boolean NOT NULL)",
    "CREATE TABLE decision_citation (citation_id bigint PRIMARY KEY, run_id integer NOT NULL REFERENCES extract_run, \
     decision_id bigint NOT NULL REFERENCES decision, section_id bigint NOT NULL REFERENCES decision_section, \
     char_start integer NOT NULL, char_end integer NOT NULL, raw text NOT NULL, law_id text, article_key text, \
     unit_path text, article_version_id text, relation text NOT NULL, resolution text NOT NULL)",
    "CREATE TABLE decision_overrule (overrule_id bigint PRIMARY KEY, run_id integer NOT NULL REFERENCES extract_run, \
     overruling_decision_id bigint NOT NULL REFERENCES decision, overruled_case_no text NOT NULL, \
     overruled_decision_id bigint REFERENCES decision, source text NOT NULL, raw text NOT NULL)",
    "CREATE TABLE section_span (span_id bigint PRIMARY KEY, run_id integer NOT NULL REFERENCES extract_run, \
     section_id bigint NOT NULL REFERENCES decision_section, char_start integer NOT NULL, char_end integer NOT NULL, \
     kind text NOT NULL)",
];

pub const REASONS_HEAD: &str = "원심은 피고 소속 공무원의 과실을 부정하였다. ";
pub const REASONS_TAIL: &str = "그러나 피고 소속 공무원의 과실이 인정되므로 배상 책임이 있다.";
pub const SUMMARY_10: &str = "공무원의 직무상 과실로 인한 국가배상 책임을 인정한 사례.";
pub const ISSUE_10: &str = "공무원의 과실을 판단하는 기준.";
pub const DISSENT_10: &str = "반대의견은 공무원 과실을 인정할 수 없다고 본다.";
pub const SUMMARY_11: &str = "처분의 절차상 하자와 담당 공무원의 과실 여부를 판단하였다.";
pub const SUMMARY_13: &str = "공무원의 과실 판단에 관한 옛 법리를 밝힌 사례.";
pub const BUNDLE_BODY: &str = "묶음 행: 공무원 과실 국가배상 사건 여럿.";

pub const VECTOR_MODEL: &str = "registry-model";

fn literal(near: bool) -> String {
    let parts: Vec<&str> = (0..retrieval::embedder::DIM)
        .map(|i| if near || i % 2 == 0 { "0.01" } else { "-0.01" })
        .collect();
    format!("'[{}]'", parts.join(","))
}

fn vector_rows() -> Vec<String> {
    vec![
        "CREATE TABLE embedding_model (model_id smallint PRIMARY KEY, name text NOT NULL UNIQUE, dim integer NOT NULL)".into(),
        "CREATE TABLE chunk_embedding (chunk_id bigint NOT NULL REFERENCES decision_chunk, \
         model_id smallint NOT NULL REFERENCES embedding_model, embedding halfvec(1024) NOT NULL, PRIMARY KEY (chunk_id, model_id))"
            .into(),
        format!("INSERT INTO embedding_model VALUES (1,'{VECTOR_MODEL}',1024)"),
        format!("INSERT INTO chunk_embedding VALUES (1001,1,{}), (1000,1,{})", literal(true), literal(false)),
    ]
}

fn rows() -> Vec<String> {
    let head = REASONS_HEAD.chars().count();
    let total = head + REASONS_TAIL.chars().count();
    vec![
        "INSERT INTO law VALUES ('L1','law','국가배상법','법률'), ('L2','law','국가배상법 시행령','대통령령')".into(),
        "INSERT INTO article_version (row_id, article_version_id, article_key, law_id, article_no) VALUES \
         (1,'L1:0002@2017-01-01','L1:0002','L1','제2조'), (2,'L1:0002@2023-01-01','L1:0002','L1','제2조'), \
         (3,'L1:0003@2023-01-01','L1:0003','L1','제3조'), (4,'L2:0003@2023-01-01','L2:0003','L2','제3조')"
            .into(),
        "INSERT INTO court VALUES (1,'supreme','대법원','supreme'), (2,'seoul-high','서울고등법원','high')".into(),
        "INSERT INTO section_role VALUES ('holding_issue','판시사항'), ('holding_summary','판결요지'), ('reasons','이유'), \
         ('ref_statutes','참조조문')"
            .into(),
        "INSERT INTO decision VALUES (10,1,'2020다1111','2020다1111','2021-03-04','civil',false,true,'reverse_remand'), \
         (11,2,'2022누2222','2022누2222','2023-05-06','administrative',false,false,'reject'), \
         (12,1,'2021다3333','2021다3333 등','2022-01-01','civil',true,false,'other'), \
         (13,1,'2019다4444','2019다4444','2019-02-01','civil',false,false,'dismiss_appeal')"
            .into(),
        format!(
            "INSERT INTO decision_section VALUES \
             (105,10,1,'holding_issue','판시사항','{ISSUE_10}','majority','negative'), \
             (100,10,2,'holding_summary','판결요지','{SUMMARY_10}','majority',NULL), \
             (101,10,3,'reasons','이유','{REASONS_HEAD}{REASONS_TAIL}','majority',NULL), \
             (104,10,4,'reasons','반대의견','{DISSENT_10}','dissent',NULL), \
             (106,10,5,'ref_statutes','참조조문','국가배상법 제2조, 국가배상법 시행령 제3조',NULL,NULL), \
             (102,11,1,'holding_summary','판결요지','{SUMMARY_11}','majority',NULL), \
             (103,12,1,'holding_summary',NULL,'{BUNDLE_BODY}','majority',NULL), \
             (107,13,1,'holding_summary','판결요지','{SUMMARY_13}','majority',NULL)"
        ),
        format!("INSERT INTO decision_chunk VALUES (1000,101,1,0,{head}), (1001,101,2,{head},{total})"),
        "INSERT INTO extract_run VALUES (1,'citation','v1',false), (2,'citation','v2',true), \
         (3,'case_meta','m0',false), (4,'case_meta','m1',true)"
            .into(),
        "INSERT INTO decision_citation VALUES \
         (1,2,10,106,0,9,'국가배상법 제2조','L1','L1:0002','제2조','L1:0002@2017-01-01','cites','조문 일치'), \
         (2,2,11,102,0,5,'같은 법 제2조 제1항','L1','L1:0002','제2조제1항',NULL,'cites','조문 일치, 판본 없음'), \
         (3,1,11,102,0,5,'옛 추출 결과','L1','L1:0003','제3조','L1:0003@2023-01-01','cites','옛 판'), \
         (4,2,10,106,11,22,'국가배상법 시행령 제3조','L2','L2:0003','제3조','L2:0003@2023-01-01','cites','조문 일치'), \
         (5,2,13,107,0,5,'국가배상법 제2조 제11항','L1','L1:0002','제2조제11항',NULL,'cites','조문 일치'), \
         (6,2,13,107,6,9,'없는법 제9조',NULL,NULL,'제9조',NULL,'cites','법령명 찾지 못함')"
            .into(),
        "INSERT INTO decision_overrule VALUES \
         (1,4,10,'2019다4444',13,'reasons','대법원 2019. 2. 1. 선고 2019다4444 판결'), \
         (2,4,10,'2019다4444',13,'ref_cases','대법원 2019. 2. 1. 선고 2019다4444 판결(변경)'), \
         (3,3,10,'2022누2222',11,'reasons','옛 판 결과')"
            .into(),
        format!(
            "INSERT INTO section_span VALUES (1,4,101,0,{head},'lower_court_view'), (2,3,101,{head},{total},'lower_court_view')"
        ),
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
    open_with(false).await
}

pub async fn open_vectors() -> Result<Option<Fixture>, Failure> {
    open_with(true).await
}

async fn open_with(vectors: bool) -> Result<Option<Fixture>, Failure> {
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
    match fx.fill(&base, vectors).await {
        Ok(url) => Ok(Some(Fixture { url, ..fx })),
        Err(e) => {
            fx.close().await?;
            Err(e)
        }
    }
}

impl Fixture {
    async fn fill(&self, base: &str, vectors: bool) -> Result<String, Failure> {
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
        let vector_schema: Option<String> = if vectors {
            sqlx::query_scalar(
                "SELECT extnamespace::regnamespace::text FROM pg_extension WHERE extname = 'vector'",
            )
            .fetch_optional(&self.admin)
            .await?
        } else {
            None
        };
        if vectors && vector_schema.is_none() {
            return Err("vector extension is not installed in this database".into());
        }
        let mut path = format!("{},{}", self.schema, ext_schema);
        if let Some(v) = &vector_schema
            && !path.split(',').any(|p| p == v)
        {
            path.push(',');
            path.push_str(v);
        }
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
        if vector_schema.is_some() {
            for stmt in vector_rows() {
                sqlx::query(AssertSqlSafe(stmt)).execute(&mut *tx).await?;
            }
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
