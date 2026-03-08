use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::keyspace::Keyspace;
use crate::scope::RequestScope;
use crate::storage::StorageAdapter;

mod types;
pub use types::*;

/// Runtime-tunable knobs for `ThreadStore`.
#[derive(Clone, Debug)]
pub struct ThreadStoreConfig {
    pub cache_ttl_secs: u64,
    pub max_events: usize,
}

impl Default for ThreadStoreConfig {
    fn default() -> Self {
        Self {
            cache_ttl_secs: 5,
            max_events: 200,
        }
    }
}

impl std::fmt::Debug for ThreadStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThreadStore { .. }")
    }
}

#[derive(Clone)]
pub struct ThreadStore {
    storage: Arc<dyn StorageAdapter>,
    scope: RequestScope,
    keyspace: Arc<dyn Keyspace>,
    cache: Arc<DashMap<String, CacheEntry>>,
    config: ThreadStoreConfig,
}

#[derive(Clone)]
pub(crate) struct CacheEntry {
    pub(crate) log: ThreadLog,
    pub(crate) ts: Instant,
}

mod materialization;
mod projection;
mod store_io;

fn build_thread_events_from_log(log: &ThreadLog, max_events: usize) -> Vec<ThreadEvent> {
    projection::build_thread_events_from_log(log, max_events)
}

#[cfg(test)]
mod tests;
