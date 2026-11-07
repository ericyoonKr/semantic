use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::{self, error::TrySendError};
use tokio_util::sync::CancellationToken;

use crate::error::ApiError;
use crate::judge::JudgeGate;
use crate::session::actor::{self, SessionEvent};
use crate::session::id::SessionId;
use crate::state::AppState;

#[derive(Clone)]
pub struct SessionHandle {
    pub tx: mpsc::Sender<SessionEvent>,
    pub cancel: CancellationToken,
    pub done: CancellationToken,
    pub judges: Arc<JudgeGate>,
    id: u64,
}

#[derive(Default)]
pub struct Registry {
    map: Mutex<HashMap<SessionId, SessionHandle>>,
    next: AtomicU64,
}

impl Registry {
    pub fn get_or_spawn(
        &self,
        state: &AppState,
        sid: &SessionId,
    ) -> Result<SessionHandle, ApiError> {
        if state.shutdown.is_cancelled() {
            return Err(ApiError::closed());
        }
        let mut map = self.map.lock().map_err(|_| ApiError::internal())?;
        if let Some(h) = map.get(sid) {
            return Ok(h.clone());
        }
        let (tx, rx) = mpsc::channel(state.cfg.limits.session_channel);
        let cancel = state.shutdown.child_token();
        let handle = SessionHandle {
            tx,
            judges: Arc::new(JudgeGate::new(cancel.clone())),
            cancel,
            done: CancellationToken::new(),
            id: self.next.fetch_add(1, Ordering::Relaxed),
        };
        map.insert(sid.clone(), handle.clone());
        drop(map);
        state.tracker.spawn(actor::run(
            state.clone(),
            sid.clone(),
            rx,
            handle.cancel.clone(),
            handle.done.clone(),
            handle.id,
        ));
        Ok(handle)
    }

    pub fn remove_if(&self, sid: &SessionId, id: u64) -> bool {
        let Ok(mut map) = self.map.lock() else {
            return false;
        };
        if map.get(sid).is_some_and(|h| h.id == id) {
            map.remove(sid);
            true
        } else {
            false
        }
    }

    pub fn take(&self, sid: &SessionId) -> Option<SessionHandle> {
        self.map.lock().ok()?.remove(sid)
    }

    pub fn len(&self) -> usize {
        self.map.lock().map_or(0, |m| m.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub fn send(state: &AppState, sid: &SessionId, mut event: SessionEvent) -> Result<(), ApiError> {
    for _ in 0..2 {
        let handle = state.sessions.get_or_spawn(state, sid)?;
        let mut gate = handle.judges.lock().ok_or_else(ApiError::internal)?;
        let revises = event.revises();
        event.bind(&gate.ticket());
        match handle.tx.try_send(event) {
            Ok(()) => {
                if revises {
                    gate.revise();
                }
                return Ok(());
            }
            Err(TrySendError::Full(_)) => return Err(ApiError::busy()),
            Err(TrySendError::Closed(back)) => {
                state.sessions.remove_if(sid, handle.id);
                event = back;
            }
        }
    }
    Err(ApiError::closed())
}
