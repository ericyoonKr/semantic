use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::store::AuthStore;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub store: AuthStore,
    pub http: reqwest::Client,
    pub limiter: Arc<LoginLimiter>,
}

#[derive(Debug)]
pub struct LoginLimiter {
    per_minute: u32,
    seen: Mutex<HashMap<IpAddr, (Instant, u32)>>,
}

impl LoginLimiter {
    pub fn new(per_minute: u32) -> Self {
        Self { per_minute, seen: Mutex::new(HashMap::new()) }
    }

    pub fn allow(&self, ip: IpAddr) -> bool {
        let Ok(mut seen) = self.seen.lock() else {
            return false;
        };
        let now = Instant::now();
        seen.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(60));
        let entry = seen.entry(ip).or_insert((now, 0));
        entry.1 += 1;
        entry.1 <= self.per_minute
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
