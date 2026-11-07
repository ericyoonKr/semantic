use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::error::ApiError;
use crate::session::id::SessionId;
use crate::state::AppState;
use crate::upload::{IncomingFile, UploadReply, pipeline};

pub enum SessionEvent {
    Upload {
        file: IncomingFile,
        reply: oneshot::Sender<Result<UploadReply, ApiError>>,
        span: tracing::Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exit {
    Cancelled,
    Closed,
    Idle,
}

pub(crate) async fn run(
    state: AppState,
    sid: SessionId,
    mut rx: mpsc::Receiver<SessionEvent>,
    cancel: CancellationToken,
    done: CancellationToken,
    id: u64,
) {
    let idle = state.cfg.limits.session_idle;
    let exit = loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break Exit::Cancelled,
            got = tokio::time::timeout(idle, rx.recv()) => match got {
                Ok(Some(event)) => handle(&state, &sid, event).await,
                Ok(None) => break Exit::Closed,
                Err(_) => {
                    if state.sessions.remove_if(&sid, id) {
                        break Exit::Idle;
                    }
                }
            },
        }
    };
    rx.close();
    while let Ok(event) = rx.try_recv() {
        if exit == Exit::Idle {
            handle(&state, &sid, event).await;
        } else {
            reject(event);
        }
    }
    state.sessions.remove_if(&sid, id);
    tracing::debug!(exit = ?exit, "session actor stopped");
    done.cancel();
}

async fn handle(state: &AppState, sid: &SessionId, event: SessionEvent) {
    match event {
        SessionEvent::Upload { file, reply, span } => {
            let out = pipeline::run(state, sid, file).instrument(span).await;
            let _ = reply.send(out);
        }
    }
}

fn reject(event: SessionEvent) {
    match event {
        SessionEvent::Upload { reply, .. } => {
            let _ = reply.send(Err(ApiError::closed()));
        }
    }
}
