use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::audit::AuditStore;
use crate::config::Config;
use crate::memory::MemoryGuard;
use crate::quota::AskQuota;
use crate::store::AuthStore;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub store: AuthStore,
    pub http: reqwest::Client,
    pub limiter: Arc<LoginLimiter>,
    pub signup_limiter: Arc<LoginLimiter>,
    pub audit: AuditStore,
    pub quota: Arc<AskQuota>,
    pub memory: Arc<MemoryGuard>,
}

#[derive(Debug)]
pub struct LoginLimiter {
    limit: u32,
    window: Duration,
    seen: Mutex<HashMap<IpAddr, (Instant, u32)>>,
}

impl LoginLimiter {
    pub fn new(per_minute: u32) -> Self {
        Self::within(per_minute, Duration::from_secs(60))
    }

    pub fn within(limit: u32, window: Duration) -> Self {
        Self { limit, window, seen: Mutex::new(HashMap::new()) }
    }

    pub fn allow(&self, ip: IpAddr) -> bool {
        let Ok(mut seen) = self.seen.lock() else {
            return false;
        };
        let now = Instant::now();
        seen.retain(|_, (start, _)| now.duration_since(*start) < self.window);
        let entry = seen.entry(ip).or_insert((now, 0));
        entry.1 += 1;
        entry.1 <= self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eleventh_attempt_in_a_minute_is_refused() {
        let l = LoginLimiter::new(10);
        let ip: IpAddr = [127, 0, 0, 1].into();
        assert!((0..10).all(|_| l.allow(ip)));
        assert!(!l.allow(ip));
        assert!(l.allow([127, 0, 0, 2].into()));
    }
}
