use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use chrono::{FixedOffset, NaiveDate, Utc};
use engine::{ClaimGraph, JudgeRequest, NodeId, UserFact};
use serde::Deserialize;

use crate::error::ApiError;
use crate::rules::RuleSet;
use crate::session::SessionId;

#[derive(Debug, Deserialize)]
pub struct AssumptionBody {
    pub node_id: String,
    pub value: bool,
    #[serde(default)]
    pub said: String,
}

#[derive(Debug, Deserialize)]
pub struct UserFactBody {
    pub fact_id: String,
    pub text: String,
    pub message_id: String,
}

#[derive(Debug, Deserialize)]
pub struct JudgeBody {
    pub claim_type: String,
    pub node_id: String,
    #[serde(default)]
    pub assumptions: Vec<AssumptionBody>,
    #[serde(default)]
    pub user_facts: Vec<UserFactBody>,
}

pub struct JudgeJob {
    pub tree: Arc<ClaimGraph>,
    pub request: JudgeRequest,
    pub said: HashMap<NodeId, String>,
    pub deadline_at: Instant,
}

fn bad(reason: &'static str) -> ApiError {
    ApiError::bad_request(reason)
}

type Assumed = (HashMap<NodeId, bool>, HashMap<NodeId, String>);

fn assumptions(scope: &ClaimGraph, given: Vec<AssumptionBody>) -> Result<Assumed, ApiError> {
    let mut values = HashMap::new();
    let mut said = HashMap::new();
    for a in given {
        let id = NodeId::new(a.node_id.trim());
        if !scope.contains(&id) {
            return Err(bad("가정한 노드가 판정 범위 밖에 있습니다"));
        }
        if values.insert(id.clone(), a.value).is_some_and(|v| v != a.value) {
            return Err(bad("같은 노드에 서로 다른 가정 값이 있습니다"));
        }
        let words = a.said.trim();
        if !words.is_empty() {
            said.insert(id, words.to_string());
        }
    }
    Ok((values, said))
}

fn user_facts(given: Vec<UserFactBody>) -> Result<Vec<UserFact>, ApiError> {
    let mut seen = HashSet::new();
    given
        .into_iter()
        .map(|f| {
            let fact_id = f.fact_id.trim().to_string();
            let message_id = f.message_id.trim().to_string();
            if f.text.trim().is_empty() {
                return Err(bad("빈 사용자 진술이 있습니다"));
            }
            if fact_id.is_empty() || !seen.insert(fact_id.clone()) {
                return Err(bad("사용자 진술 번호(fact_id)가 비었거나 겹칩니다"));
            }
            if message_id.is_empty() {
                return Err(bad("사용자 진술의 메시지 번호(message_id)가 없습니다"));
            }
            Ok(UserFact {
                fact_id,
                message_id,
                text: f.text,
            })
        })
        .collect()
}

const SEOUL_SECS: i32 = 9 * 3600;

pub fn today_in_seoul() -> NaiveDate {
    let now = Utc::now();
    match FixedOffset::east_opt(SEOUL_SECS) {
        Some(offset) => now.with_timezone(&offset).date_naive(),
        None => now.date_naive(),
    }
}

pub fn prepare(
    rules: &RuleSet,
    sid: &SessionId,
    body: JudgeBody,
    deadline_at: Instant,
) -> Result<JudgeJob, ApiError> {
    let tree = rules
        .tree(body.claim_type.trim())
        .ok_or_else(|| bad("알 수 없는 청구 유형입니다 — 요건 카탈로그에서 골라 주세요"))?;
    let root = NodeId::new(body.node_id.trim());
    let scope = tree
        .subtree(&root)
        .map_err(|_| bad("그 청구 유형의 요건 트리에 없는 노드입니다"))?;
    let (values, said) = assumptions(&scope, body.assumptions)?;
    Ok(JudgeJob {
        request: JudgeRequest {
            session: sid.as_str().to_string(),
            root,
            assumptions: values,
            user_facts: user_facts(body.user_facts)?,
            today: today_in_seoul(),
        },
        tree,
        said,
        deadline_at,
    })
}
