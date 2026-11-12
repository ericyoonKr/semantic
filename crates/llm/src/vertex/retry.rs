use std::future::Future;
use std::hash::{BuildHasher, RandomState};
use std::time::Duration;

use super::error::{self, VertexError};

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub retries: u32,
    pub base: Duration,
    pub cap: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self { retries: 5, base: Duration::from_millis(500), cap: Duration::from_secs(30) }
    }
}

fn jitter_fraction() -> f64 {
    let n = RandomState::new().hash_one(std::time::SystemTime::now());
    0.5 + (n % 1000) as f64 / 2000.0
}

impl RetryPolicy {
    pub fn wait(&self, attempt: u32, hint: Option<Duration>) -> Duration {
        let exp = self.base.saturating_mul(2u32.saturating_pow(attempt.min(16))).min(self.cap);
        let jittered = exp.mul_f64(jitter_fraction());
        match hint {
            Some(h) => h.max(jittered).min(self.cap.max(h)),
            None => jittered,
        }
    }

    pub async fn run<T, F, Fut>(&self, model: &str, op: &str, mut call: F) -> Result<T, VertexError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, VertexError>>,
    {
        let mut attempt = 0u32;
        loop {
            match call().await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    error::log(&e, model, op, attempt);
                    if !e.is_retryable() || attempt >= self.retries {
                        return Err(e);
                    }
                    tokio::time::sleep(self.wait(attempt, e.retry_after())).await;
                    attempt += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RetryPolicy;
    use crate::vertex::error::VertexError;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    #[test]
    fn waits_grow_stay_under_cap_and_respect_hint() {
        let p = RetryPolicy { retries: 3, base: Duration::from_millis(100), cap: Duration::from_secs(1) };
        let w0 = p.wait(0, None);
        assert!(w0 >= Duration::from_millis(50) && w0 <= Duration::from_millis(100), "{w0:?}");
        assert!(p.wait(10, None) <= Duration::from_secs(1));
        assert!(p.wait(0, Some(Duration::from_secs(3))) >= Duration::from_secs(3));
    }

    #[tokio::test]
    async fn retries_only_retryable_errors() {
        let p = RetryPolicy { retries: 2, base: Duration::from_millis(1), cap: Duration::from_millis(2) };
        let n = AtomicU32::new(0);
        let r: Result<u32, VertexError> = p
            .run("m", "t", || {
                let k = n.fetch_add(1, Ordering::SeqCst);
                async move { if k < 2 { Err(VertexError::Connect) } else { Ok(k) } }
            })
            .await;
        assert_eq!(r, Ok(2));
        let m = AtomicU32::new(0);
        let r2: Result<u32, VertexError> = p
            .run("m", "t", || {
                m.fetch_add(1, Ordering::SeqCst);
                async { Err(VertexError::Auth { code: 403, status: String::new(), message: String::new() }) }
            })
            .await;
        assert!(r2.is_err());
        assert_eq!(m.load(Ordering::SeqCst), 1);
    }
}
