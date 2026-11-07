use llm::LlmError;

use super::ask::{AskFailure, Asked, LeafAnswer};
use crate::deps::FactProvider;
use crate::schema::{FailureReason, LeafResult, Trit};
use crate::slices::{FactDocument, LeafRequest};

pub const CANCELLED: &str = "판정이 취소됨";
pub const ABORTED: &str = "판정 작업이 끝나지 않음";
pub const LOCATE_FAILED: &str = "인용 위치를 확인하지 못함";
pub const MODEL_FOUND_NOTHING: &str = "사실 자료에서 판단 근거를 찾지 못함";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Settled {
    pub result: LeafResult,
    pub store: bool,
}

impl Settled {
    pub fn kept(result: LeafResult) -> Self {
        Self {
            result,
            store: true,
        }
    }

    pub fn unknown(failure: FailureReason, store: bool) -> Self {
        Self {
            result: LeafResult::Unknown { failure },
            store,
        }
    }

    pub fn is_true(&self) -> bool {
        is_true(&self.result)
    }
}

pub(crate) fn is_true(result: &LeafResult) -> bool {
    matches!(result, LeafResult::Judged { value: true, .. })
}

pub(crate) async fn settle<F: FactProvider>(
    asked: Asked,
    req: &LeafRequest,
    facts: &F,
    session: &str,
) -> Settled {
    match asked {
        Ok(answer) => from_answer(answer, req, facts, session).await,
        Err(failure) => from_failure(failure),
    }
}

fn from_failure(failure: AskFailure) -> Settled {
    match failure {
        AskFailure::Llm(LlmError::Timeout) => Settled::unknown(FailureReason::LlmTimeout, false),
        AskFailure::Llm(LlmError::ParseFailed) => {
            Settled::unknown(FailureReason::LlmParseFailed, true)
        }
        AskFailure::Llm(LlmError::Cancelled) | AskFailure::Cancelled => {
            Settled::unknown(FailureReason::Cancelled(CANCELLED.to_string()), false)
        }
        AskFailure::Llm(LlmError::Unavailable | LlmError::RateLimited | LlmError::Http(_)) => {
            Settled::unknown(FailureReason::ModelServerDown, false)
        }
        AskFailure::Aborted => {
            Settled::unknown(FailureReason::Cancelled(ABORTED.to_string()), false)
        }
    }
}

async fn from_answer<F: FactProvider>(
    answer: LeafAnswer,
    req: &LeafRequest,
    facts: &F,
    session: &str,
) -> Settled {
    let value = match answer.value {
        Trit::True => true,
        Trit::False => false,
        Trit::Unknown => {
            return Settled::unknown(
                FailureReason::NoEvidence(MODEL_FOUND_NOTHING.to_string()),
                true,
            );
        }
    };
    let quote = answer.quote.trim();
    if quote.is_empty() {
        return Settled::unknown(FailureReason::QuoteNotFound, true);
    }
    let source = answer.source.trim();
    let located = if source == "doc" {
        if req.facts.document == FactDocument::Absent {
            Ok(None)
        } else {
            facts.locate(session, quote).await
        }
    } else if let Some(fact_id) = source.strip_prefix("user:") {
        Ok(req
            .facts
            .user_facts
            .iter()
            .find(|f| f.fact_id == fact_id.trim())
            .and_then(|f| facts.locate_user(f, quote)))
    } else {
        Ok(None)
    };
    match located {
        Ok(Some(anchor)) => Settled::kept(LeafResult::Judged {
            value,
            anchor,
            quote: quote.to_string(),
        }),
        Ok(None) => Settled::unknown(FailureReason::QuoteNotFound, true),
        Err(error) => {
            tracing::warn!(node = %req.node_id, %error, "engine.leaf.locate");
            Settled::unknown(FailureReason::NoEvidence(LOCATE_FAILED.to_string()), false)
        }
    }
}
