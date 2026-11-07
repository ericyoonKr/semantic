use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::LlmError;

#[derive(Clone, Debug)]
pub struct LlmLimiter {
    semaphore: Arc<Semaphore>,
    permits: Arc<AtomicUsize>,
}

impl Default for LlmLimiter {
    fn default() -> Self {
        Self::new(8)
    }
}

impl LlmLimiter {
    pub fn new(permits: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(permits)),
            permits: Arc::new(AtomicUsize::new(permits)),
        }
    }

    pub fn permits(&self) -> usize {
        self.permits.load(Ordering::SeqCst)
    }

    pub fn available(&self) -> usize {
        self.semaphore.available_permits()
    }

    pub async fn acquire(&self) -> Result<OwnedSemaphorePermit, LlmError> {
        self.semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| LlmError::Cancelled)
    }

    pub fn set_permits(&self, target: usize) {
        let current = self.permits.load(Ordering::SeqCst);
        if target > current {
            self.semaphore.add_permits(target - current);
            self.permits.store(target, Ordering::SeqCst);
        } else if target < current {
            let removed = self.semaphore.forget_permits(current - target);
            self.permits.store(current - removed, Ordering::SeqCst);
        }
    }
}

#[derive(Debug)]
pub struct TieredPermit {
    _local: OwnedSemaphorePermit,
    _global: OwnedSemaphorePermit,
}

pub async fn acquire_tiered(local: &LlmLimiter, global: &LlmLimiter) -> Result<TieredPermit, LlmError> {
    let local_permit = local.acquire().await?;
    let global_permit = global.acquire().await?;
    Ok(TieredPermit {
        _local: local_permit,
        _global: global_permit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn set_permits_lowers_and_raises_capacity() -> Result<(), LlmError> {
        let limiter = LlmLimiter::new(4);
        limiter.set_permits(2);
        assert_eq!(limiter.permits(), 2);
        assert_eq!(limiter.available(), 2);
        let held = limiter.acquire().await?;
        limiter.set_permits(6);
        assert_eq!(limiter.permits(), 6);
        assert_eq!(limiter.available(), 5);
        drop(held);
        assert_eq!(limiter.available(), 6);
        Ok(())
    }
}
