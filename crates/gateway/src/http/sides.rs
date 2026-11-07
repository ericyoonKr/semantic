use uuid::Uuid;

use crate::audit::{AuditStore, Event};
use crate::filter::SideEvent;
use crate::store::AuthStore;

pub struct SideSink {
    pub store: AuthStore,
    pub audit: AuditStore,
    pub user: Uuid,
    pub thread: Uuid,
    pub request_id: String,
}

impl SideSink {
    pub fn handle(&self, side: SideEvent) {
        match side {
            SideEvent::Usage { tokens } => {
                let store = self.store.clone();
                let user = self.user;
                let request_id = self.request_id.clone();
                tokio::spawn(async move {
                    if let Err(e) = store.add_tokens(user, tokens).await {
                        tracing::warn!(error = %e, request_id = %request_id, "usage.write_failed");
                    }
                });
            }
            SideEvent::Audit { kind, detail } => {
                self.audit.spawn(Event::new(&kind).user(self.user).thread(self.thread).request(&self.request_id).detail(detail));
            }
        }
    }
}
