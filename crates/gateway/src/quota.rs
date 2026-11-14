use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, timeout_at};
use uuid::Uuid;

use crate::error::GatewayError;

type Slots = Mutex<HashMap<Uuid, Arc<Semaphore>>>;

#[derive(Debug)]
pub struct AskQuota {
    global: Arc<Semaphore>,
    users: Slots,
    threads: Slots,
    wait: Duration,
    queue_wait: Duration,
}

#[derive(Debug)]
pub struct AskPermit {
    _global: OwnedSemaphorePermit,
    _user: OwnedSemaphorePermit,
    _thread: OwnedSemaphorePermit,
}

fn slot(slots: &Slots, key: Uuid) -> Result<Arc<Semaphore>, GatewayError> {
    let mut map = slots.lock().map_err(|_| GatewayError::Internal)?;
    map.retain(|_, s| Arc::strong_count(s) > 1);
    Ok(map.entry(key).or_insert_with(|| Arc::new(Semaphore::new(1))).clone())
}

async fn wait_for(slot: Arc<Semaphore>, deadline: Instant, late: GatewayError) -> Result<OwnedSemaphorePermit, GatewayError> {
    match timeout_at(deadline, slot.acquire_owned()).await {
        Ok(Ok(permit)) => Ok(permit),
        Ok(Err(_)) => Err(GatewayError::Internal),
        Err(_) => Err(late),
    }
}

impl AskQuota {
    pub fn new(global: usize, wait: Duration, queue_wait: Duration) -> Self {
        Self { global: Arc::new(Semaphore::new(global)), users: Mutex::new(HashMap::new()), threads: Mutex::new(HashMap::new()), wait, queue_wait }
    }

    pub fn try_acquire(&self, user: Uuid, thread: Uuid) -> Result<Option<AskPermit>, GatewayError> {
        let thread_slot = slot(&self.threads, thread)?;
        let user_slot = slot(&self.users, user)?;
        let Ok(thread_permit) = thread_slot.try_acquire_owned() else { return Ok(None) };
        let Ok(user_permit) = user_slot.try_acquire_owned() else { return Ok(None) };
        let Ok(global_permit) = self.global.clone().try_acquire_owned() else { return Ok(None) };
        Ok(Some(AskPermit { _global: global_permit, _user: user_permit, _thread: thread_permit }))
    }

    pub async fn acquire(&self, user: Uuid, thread: Uuid) -> Result<AskPermit, GatewayError> {
        let thread_slot = slot(&self.threads, thread)?;
        let user_slot = slot(&self.users, user)?;
        let queue_deadline = Instant::now() + self.queue_wait;
        let thread_permit = wait_for(thread_slot, queue_deadline, GatewayError::Busy).await?;
        let user_permit = wait_for(user_slot, queue_deadline, GatewayError::TooMany).await?;
        let global_permit = wait_for(self.global.clone(), Instant::now() + self.wait, GatewayError::TooMany).await?;
        Ok(AskPermit { _global: global_permit, _user: user_permit, _thread: thread_permit })
    }

    pub fn available(&self) -> usize {
        self.global.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quota() -> AskQuota {
        AskQuota::new(4, Duration::from_millis(200), Duration::from_millis(200))
    }

    #[tokio::test]
    async fn same_thread_waits_for_the_running_answer_then_gets_in() {
        let q = Arc::new(quota());
        let (u, t) = (Uuid::new_v4(), Uuid::new_v4());
        let first = q.acquire(u, t).await;
        assert!(first.is_ok());
        assert!(q.try_acquire(u, t).ok().flatten().is_none());
        let started = Instant::now();
        assert!(matches!(q.acquire(Uuid::new_v4(), t).await, Err(GatewayError::Busy)));
        assert!(started.elapsed() >= Duration::from_millis(200));
        let q2 = q.clone();
        let waiter = tokio::spawn(async move { q2.acquire(u, t).await.is_ok() });
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(first);
        assert!(matches!(waiter.await, Ok(true)));
        assert!(q.try_acquire(u, t).ok().flatten().is_some());
    }

    #[tokio::test]
    async fn second_ask_of_a_user_waits_then_is_refused() {
        let q = quota();
        let u = Uuid::new_v4();
        let first = q.acquire(u, Uuid::new_v4()).await;
        assert!(first.is_ok());
        assert!(q.try_acquire(u, Uuid::new_v4()).ok().flatten().is_none());
        let started = Instant::now();
        assert!(matches!(q.acquire(u, Uuid::new_v4()).await, Err(GatewayError::TooMany)));
        assert!(started.elapsed() >= Duration::from_millis(200));
        assert!(q.acquire(Uuid::new_v4(), Uuid::new_v4()).await.is_ok());
    }

    #[tokio::test]
    async fn waiting_user_gets_in_when_first_finishes() {
        let q = Arc::new(AskQuota::new(4, Duration::from_secs(5), Duration::from_secs(5)));
        let u = Uuid::new_v4();
        let first = q.acquire(u, Uuid::new_v4()).await;
        assert!(first.is_ok());
        let q2 = q.clone();
        let waiter = tokio::spawn(async move { q2.acquire(u, Uuid::new_v4()).await.is_ok() });
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(first);
        assert!(matches!(waiter.await, Ok(true)));
    }

    #[tokio::test]
    async fn fifth_ask_system_wide_is_refused_after_wait() {
        let q = quota();
        let mut held = Vec::new();
        for _ in 0..4 {
            held.push(q.acquire(Uuid::new_v4(), Uuid::new_v4()).await);
        }
        assert!(held.iter().all(Result::is_ok));
        assert_eq!(q.available(), 0);
        assert!(q.try_acquire(Uuid::new_v4(), Uuid::new_v4()).ok().flatten().is_none());
        assert!(matches!(q.acquire(Uuid::new_v4(), Uuid::new_v4()).await, Err(GatewayError::TooMany)));
        held.clear();
        assert_eq!(q.available(), 4);
    }
}
