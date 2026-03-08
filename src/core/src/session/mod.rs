use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::keyspace::Keyspace;
use crate::scope::RequestScope;
use crate::storage::StorageAdapter;

mod types;
pub use types::*;

mod control_state;
pub use control_state::ControlStateStore;

mod log_reader;
pub use log_reader::ThreadLogReader;

mod log_writer;
pub use log_writer::ThreadLogWriter;

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
pub use materialization::apply_step_to_state;

mod projection;
pub use projection::build_thread_events_from_log;

mod store_io;

#[async_trait::async_trait]
impl ThreadLogReader for ThreadStore {
    async fn get_log(&self, thread_id: &str) -> crate::error::CoreResult<ThreadLog> {
        self.get(thread_id).await
    }
    async fn get_events(&self, thread_id: &str, max: usize) -> crate::error::CoreResult<Vec<ThreadEvent>> {
        let log = self.get(thread_id).await?;
        Ok(build_thread_events_from_log(&log, max))
    }
    async fn get_step_count(&self, thread_id: &str) -> crate::error::CoreResult<usize> {
        let log = self.get(thread_id).await?;
        Ok(log.steps.len())
    }
    async fn list_thread_ids(&self) -> Vec<String> {
        self.list().await
    }
}

#[cfg(test)]
mod tests;
