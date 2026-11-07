use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[cfg(windows)]
mod sys {
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }

    pub fn available_bytes() -> Option<u64> {
        let mut status = MemoryStatusEx {
            length: std::mem::size_of::<MemoryStatusEx>() as u32,
            memory_load: 0,
            total_phys: 0,
            avail_phys: 0,
            total_page_file: 0,
            avail_page_file: 0,
            total_virtual: 0,
            avail_virtual: 0,
            avail_extended_virtual: 0,
        };
        let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
        (ok != 0).then_some(status.avail_phys)
    }
}

#[cfg(target_os = "linux")]
mod sys {
    pub fn available_bytes() -> Option<u64> {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let line = text.lines().find(|l| l.starts_with("MemAvailable:"))?;
        let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kib * 1024)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod sys {
    pub fn available_bytes() -> Option<u64> {
        None
    }
}

pub use sys::available_bytes;

#[derive(Debug)]
pub struct MemoryGuard {
    floor_bytes: u64,
    low: AtomicBool,
}

impl MemoryGuard {
    pub fn new(floor_bytes: u64) -> Self {
        Self { floor_bytes, low: AtomicBool::new(false) }
    }

    pub fn update(&self, available: Option<u64>) -> bool {
        let low = available.is_some_and(|a| a < self.floor_bytes);
        let was = self.low.swap(low, Ordering::Relaxed);
        if low && !was {
            tracing::warn!(available_mib = available.unwrap_or(0) / (1024 * 1024), "memory.low");
        } else if !low && was {
            tracing::info!("memory.recovered");
        }
        low
    }

    pub fn low(&self) -> bool {
        self.low.load(Ordering::Relaxed)
    }

    pub fn spawn_watch(self: &Arc<Self>, every: Duration) {
        let guard = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            loop {
                tick.tick().await;
                let available = tokio::task::spawn_blocking(available_bytes).await.ok().flatten();
                guard.update(available);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_machine_reports_available_memory() {
        let got = available_bytes();
        assert!(got.is_some_and(|b| b > 0));
    }

    #[test]
    fn below_floor_is_low_and_unknown_is_allowed() {
        let g = MemoryGuard::new(1024);
        assert!(g.update(Some(10)));
        assert!(g.low());
        assert!(!g.update(Some(4096)));
        assert!(!g.update(None));
        assert!(!g.low());
    }
}
