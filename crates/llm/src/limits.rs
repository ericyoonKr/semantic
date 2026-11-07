use std::sync::Arc;
use tokio::sync::{Semaphore, OwnedSemaphorePermit};

#[derive(Clone, Debug)]
pub struct LlmLimiter {
    semaphore: Arc<Semaphore>,
}

impl Default for LlmLimiter {
    fn default() -> Self {
        Self::new(8)
    }
}

impl LlmLimiter {

    pub fn new(permits: usize) -> Self{
        Self { semaphore: Arc::new(Semaphore::new(permits)),
        }
    }

    pub async fn acquire(&self) -> OwnedSemaphorePermit {
        self.semaphore.clone().acquire_owned().await.unwrap()
    }
}
