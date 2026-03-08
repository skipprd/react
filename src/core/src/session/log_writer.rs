use std::sync::Arc;

use crate::error::CoreResult;
use crate::keyspace::Keyspace;
use crate::scope::RequestScope;
use crate::storage::StorageAdapter;

use super::{ThreadStep, ThreadStore, ThreadStoreConfig};

/// Append-only writer for the ThreadLog.
///
/// The suite uses this to record audit events. The writer internally
/// delegates to ThreadStore (which also serves as the ThreadLogReader impl)
/// so reads after writes are cache-consistent within a process.
#[derive(Clone)]
pub struct ThreadLogWriter {
    store: ThreadStore,
}

impl std::fmt::Debug for ThreadLogWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThreadLogWriter { .. }")
    }
}

impl ThreadLogWriter {
    pub fn new(
        storage: Arc<dyn StorageAdapter>,
        scope: RequestScope,
        keyspace: Arc<dyn Keyspace>,
    ) -> Self {
        Self {
            store: ThreadStore::new(storage, scope, keyspace),
        }
    }

    pub fn with_config(
        storage: Arc<dyn StorageAdapter>,
        scope: RequestScope,
        keyspace: Arc<dyn Keyspace>,
        config: ThreadStoreConfig,
    ) -> Self {
        Self {
            store: ThreadStore::with_config(storage, scope, keyspace, config),
        }
    }

    pub fn from_store(store: ThreadStore) -> Self {
        Self { store }
    }

    pub fn as_store(&self) -> &ThreadStore {
        &self.store
    }

    pub async fn append_step(&self, thread_id: &str, step: ThreadStep) -> CoreResult<()> {
        self.store.append_step(thread_id, step).await
    }

    pub async fn set_title_if_absent(&self, thread_id: &str, title: &str) -> CoreResult<()> {
        self.store.set_title_if_absent(thread_id, title).await
    }

    pub async fn lock_title(&self, thread_id: &str, title: &str) -> CoreResult<()> {
        self.store.lock_title(thread_id, title).await
    }
}
