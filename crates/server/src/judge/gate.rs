use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct JudgeTicket {
    pub token: CancellationToken,
    revised: Arc<AtomicBool>,
}

impl JudgeTicket {
    fn under(parent: &CancellationToken) -> Self {
        Self {
            token: parent.child_token(),
            revised: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn revised(&self) -> bool {
        self.revised.load(Ordering::Acquire)
    }
}

pub struct JudgeGate {
    parent: CancellationToken,
    current: Mutex<JudgeTicket>,
}

pub struct GateGuard<'a> {
    parent: &'a CancellationToken,
    slot: MutexGuard<'a, JudgeTicket>,
}

impl JudgeGate {
    pub fn new(parent: CancellationToken) -> Self {
        let first = JudgeTicket::under(&parent);
        Self {
            parent,
            current: Mutex::new(first),
        }
    }

    pub fn lock(&self) -> Option<GateGuard<'_>> {
        let slot = self.current.lock().ok()?;
        Some(GateGuard {
            parent: &self.parent,
            slot,
        })
    }
}

impl GateGuard<'_> {
    pub fn ticket(&self) -> JudgeTicket {
        self.slot.clone()
    }

    pub fn revise(&mut self) {
        self.slot.revised.store(true, Ordering::Release);
        self.slot.token.cancel();
        *self.slot = JudgeTicket::under(self.parent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_cancels_only_tickets_handed_out_before_it() {
        let parent = CancellationToken::new();
        let gate = JudgeGate::new(parent.clone());
        let Some(mut guard) = gate.lock() else {
            assert!(gate.lock().is_some());
            return;
        };
        let before = guard.ticket();
        guard.revise();
        let after = guard.ticket();
        drop(guard);
        assert!(before.token.is_cancelled() && before.revised());
        assert!(!after.token.is_cancelled() && !after.revised());
        parent.cancel();
        assert!(after.token.is_cancelled() && !after.revised());
    }
}
