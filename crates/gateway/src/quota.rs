use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, timeout_at};
use uuid::Uuid;

use crate::error::GatewayError;

#[derive(Debug)]
pub struct AskQuota {
    global: Arc<Semaphore>,
    users: Mutex<HashMap<Uuid, Arc<Semaphore>>>,
    threads: Arc<Mutex<HashSet<Uuid>>>,
    wait: Duration,
}

#[derive(Debug)]
struct ThreadMark {
    threads: Arc<Mutex<HashSet<Uuid>>>,
    id: Uuid,
}

impl Drop for ThreadMark {
    fn drop(&mut self) {
        if let Ok(mut set) = self.threads.lock() {
            set.remove(&self.id);
        }
    }
}

#[derive(Debug)]
pub struct AskPermit {
    _global: OwnedSemaphorePermit,
    _user: OwnedSemaphorePermit,
    _thread: ThreadMark,
}

impl AskQuota {
    pub fn new(global: usize, wait: Duration) -> Self {
        Self { global: Arc::new(Semaphore::new(global)), users: Mutex::new(HashMap::new()), threads: Arc::new(Mutex::new(HashSet::new())), wait }
    }

    fn mark(&self, thread: Uuid) -> Result<ThreadMark, GatewayError> {
        let mut set = self.threads.lock().map_err(|_| GatewayError::Internal)?;
        if !set.insert(thread) {
            return Err(GatewayError::Busy);
        }
        Ok(ThreadMark { threads: self.threads.clone(), id: thread })
    }

    fn user_slot(&self, user: Uuid) -> Result<Arc<Semaphore>, GatewayError> {
        let mut users = self.users.lock().map_err(|_| GatewayError::Internal)?;
        users.retain(|_, s| Arc::strong_count(s) > 1);
        Ok(users.entry(user).or_insert_with(|| Arc::new(Semaphore::new(1))).clone())
    }

    pub async fn acquire(&self, user: Uuid, thread: Uuid) -> Result<AskPermit, GatewayError> {
        let mark = self.mark(thread)?;
        let slot = self.user_slot(user)?;
        let deadline = Instant::now() + self.wait;
        let user_permit = match timeout_at(deadline, slot.acquire_owned()).await {
            Ok(Ok(p)) => p,
            Ok(Err(_)) => return Err(GatewayError::Internal),
            Err(_) => return Err(GatewayError::TooMany),
        };
        let global_permit = match timeout_at(deadline, self.global.clone().acquire_owned()).await {
            Ok(Ok(p)) => p,
            Ok(Err(_)) => return Err(GatewayError::Internal),
            Err(_) => return Err(GatewayError::TooMany),
        };
        Ok(AskPermit { _global: global_permit, _user: user_permit, _thread: mark })
    }

    pub fn available(&self) -> usize {
        self.global.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quota() -> AskQuota {
        AskQuota::new(4, Duration::from_millis(200))
    }

    #[tokio::test]
    async fn same_thread_is_busy_at_once() {
        let q = quota();
        let (u, t) = (Uuid::new_v4(), Uuid::new_v4());
        let first = q.acquire(u, t).await;
        assert!(first.is_ok());
        let started = Instant::now();
        assert!(matches!(q.acquire(Uuid::new_v4(), t).await, Err(GatewayError::Busy)));
        assert!(started.elapsed() < Duration::from_millis(50));
        drop(first);
        assert!(q.acquire(u, t).await.is_ok());
    }

    #[tokio::test]
    async fn second_ask_of_a_user_waits_then_is_refused() {
        let q = quota();
        let u = Uuid::new_v4();
        let first = q.acquire(u, Uuid::new_v4()).await;
        assert!(first.is_ok());
        let started = Instant::now();
        assert!(matches!(q.acquire(u, Uuid::new_v4()).await, Err(GatewayError::TooMany)));
        assert!(started.elapsed() >= Duration::from_millis(200));
        assert!(q.acquire(Uuid::new_v4(), Uuid::new_v4()).await.is_ok());
    }

    #[tokio::test]
    async fn waiting_user_gets_in_when_first_finishes() {
        let q = Arc::new(AskQuota::new(4, Duration::from_secs(5)));
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
        assert!(matches!(q.acquire(Uuid::new_v4(), Uuid::new_v4()).await, Err(GatewayError::TooMany)));
        held.clear();
        assert_eq!(q.available(), 4);
    }
}
