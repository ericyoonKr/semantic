use engine::{EngineError, FailureReason, Verdict, judge};

use crate::error::ApiError;
use crate::judge::gate::JudgeTicket;
use crate::judge::reply::{JudgeReply, judged, not_run};
use crate::judge::request::JudgeJob;
use crate::state::AppState;
use crate::verdict_text::{render, unavailable};

pub const REVISED: &str = "문서 수정판 도착";
pub const SHUTTING_DOWN: &str = "서버 종료";
pub const SESSION_CLOSED: &str = "세션이 닫힘";
pub const OUT_OF_TIME: &str = "처리 시간 제한을 넘음";
pub const ENGINE_OFF: &str = "판정 기능이 꺼져 있음(모델 설정 없음)";
pub const NO_ARITHMETIC: &str = "기간 계산이 필요한 리프(산술 리프)가 든 범위는 아직 판정하지 않음";

fn why(state: &AppState, ticket: &JudgeTicket) -> &'static str {
    if ticket.revised() {
        REVISED
    } else if state.shutdown.is_cancelled() {
        SHUTTING_DOWN
    } else if ticket.token.is_cancelled() {
        SESSION_CLOSED
    } else {
        OUT_OF_TIME
    }
}

fn was_cut(v: &Verdict) -> bool {
    v.unknown_leaves
        .iter()
        .any(|(_, f)| matches!(f, FailureReason::Cancelled(_)))
}

pub async fn run(state: &AppState, job: JudgeJob, ticket: JudgeTicket) -> Result<JudgeReply, ApiError> {
    let JudgeJob {
        tree,
        request,
        said,
        arithmetic,
        deadline_at,
    } = job;
    let root = request.root.clone();
    let Some(parts) = state.engine.as_ref() else {
        return Ok(not_run(&tree, &root, unavailable(ENGINE_OFF)));
    };
    if let Some(id) = arithmetic {
        return Ok(not_run(&tree, &root, unavailable(&format!("{NO_ARITHMETIC} — {id}"))));
    }
    let run = ticket.token.child_token();
    let stop_at = deadline_at
        .checked_sub(state.cfg.judge.wrapup)
        .unwrap_or(deadline_at);
    let timer = {
        let run = run.clone();
        async move {
            tokio::time::sleep_until(stop_at.into()).await;
            run.cancel();
        }
    };
    let deps = parts.deps(&state.db, &request.session, state.corpus.clone());
    let work = async {
        let permit = tokio::select! {
            biased;
            () = run.cancelled() => None,
            p = state.z3.acquire() => p.ok(),
        };
        let _permit = permit?;
        Some(judge(&tree, request, &deps, &run).await)
    };
    tokio::pin!(work);
    let out = tokio::select! {
        out = &mut work => out,
        () = timer => (&mut work).await,
    };
    match out {
        None => Ok(not_run(&tree, &root, unavailable(why(state, &ticket)))),
        Some(Ok(verdict)) => {
            let interrupted = (run.is_cancelled() && was_cut(&verdict)).then(|| why(state, &ticket));
            tracing::info!(
                outcome = ?verdict.outcome,
                unknown = verdict.unknown_leaves.len(),
                interrupted = interrupted.is_some(),
                "judge done"
            );
            let text = render(&verdict, &tree, &said, interrupted);
            Ok(judged(&verdict, &tree, text))
        }
        Some(Err(EngineError::ArithmeticUnsupported(id))) => {
            Ok(not_run(&tree, &root, unavailable(&format!("{NO_ARITHMETIC} — {id}"))))
        }
        Some(Err(EngineError::UnknownNode(_))) => {
            Err(ApiError::bad_request("가정한 노드가 판정 범위 밖에 있습니다"))
        }
        Some(Err(e)) => {
            tracing::error!(error = %e, "judge failed");
            Err(ApiError::internal())
        }
    }
}
