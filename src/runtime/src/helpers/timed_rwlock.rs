use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

const LOCK_CONTENTION_WARN_MS: u64 = 200;

static PROFILE_PERFORMANCE: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("REACT_PROFILE_LOCKS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
});

static WAITING_ON: LazyLock<DashMap<String, Instant>> = LazyLock::new(DashMap::new);
// Known limitation: process-global singleton for lock-contention profiling.
// Acceptable for diagnostics; consider injecting via a `Profiler` context if
// multi-instance testing becomes necessary.
pub static TOTAL_WAIT_TIMES: LazyLock<DashMap<String, AtomicU64>> = LazyLock::new(DashMap::new);

pub struct TimedRwLock<T> {
    name: String,
    lock: RwLock<T>,
}

impl<T> TimedRwLock<T> {
    pub fn new(name: String, t: T) -> TimedRwLock<T> {
        TimedRwLock {
            name,
            lock: RwLock::new(t),
        }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        let start_time = if *PROFILE_PERFORMANCE {
            Some(Instant::now())
        } else {
            None
        };
        let guard = self.lock.read().unwrap();
        if let Some(start) = start_time {
            self.record_wait(start);
        }
        guard
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        let start_time = if *PROFILE_PERFORMANCE {
            Some(Instant::now())
        } else {
            None
        };
        let guard = self.lock.write().unwrap();
        if let Some(start) = start_time {
            self.record_wait(start);
        }
        guard
    }

    fn record_wait(&self, start: Instant) {
        let elapsed = start.elapsed();
        let nanos = elapsed.as_nanos() as u64;
        TOTAL_WAIT_TIMES
            .entry(self.name.clone())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(nanos, Ordering::Relaxed);
        let threshold = Duration::from_millis(LOCK_CONTENTION_WARN_MS);
        if elapsed >= threshold {
            let now = Instant::now();
            let mut should_print = true;
            if let Some(prev) = WAITING_ON.get(&self.name) {
                if now.duration_since(*prev.value()) < Duration::from_secs(1) {
                    should_print = false;
                }
            }
            if should_print {
                WAITING_ON.insert(self.name.clone(), now);
                tracing::warn!(
                    lock = %self.name,
                    elapsed_ms = elapsed.as_millis() as u64,
                    "lock contention exceeded threshold",
                );
            }
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn currently_waiting() -> Vec<(String, Duration)> {
        WAITING_ON
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().elapsed()))
            .collect()
    }

    pub fn get_total_wait_times() -> Vec<(String, Duration)> {
        let totals = TOTAL_WAIT_TIMES
            .iter()
            .map(|entry| {
                (
                    entry.key().clone(),
                    Duration::from_nanos(entry.value().load(Ordering::Relaxed)),
                )
            })
            .collect();
        TOTAL_WAIT_TIMES.clear();
        totals
    }
}
