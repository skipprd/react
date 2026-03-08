use std::sync::Arc;
use std::time::Instant;

use crate::error::{CoreError, CoreResult};

use super::{
    CacheEntry, ThreadLog, ThreadLogViewCache, ThreadStep, ThreadStore,
    ThreadStoreConfig,
    THREAD_SCHEMA_VERSION, THREAD_STATE_SCHEMA_VERSION,
};

#[derive(Clone, Copy)]
enum ThreadLogViewCacheWriteMode {
    Merge,
    Replace,
}

impl ThreadStore {
    pub fn new(
        storage: Arc<dyn crate::storage::StorageAdapter>,
        scope: crate::scope::RequestScope,
        keyspace: Arc<dyn crate::keyspace::Keyspace>,
    ) -> Self {
        Self::with_config(storage, scope, keyspace, ThreadStoreConfig::default())
    }

    pub fn with_config(
        storage: Arc<dyn crate::storage::StorageAdapter>,
        scope: crate::scope::RequestScope,
        keyspace: Arc<dyn crate::keyspace::Keyspace>,
        config: ThreadStoreConfig,
    ) -> Self {
        Self {
            storage,
            scope,
            keyspace,
            cache: Arc::new(dashmap::DashMap::new()),
            config,
        }
    }

    pub fn config(&self) -> &ThreadStoreConfig {
        &self.config
    }

    pub fn control_store(&self) -> super::ControlStateStore {
        super::ControlStateStore::new(
            self.storage.clone(),
            self.scope.clone(),
            self.keyspace.clone(),
        )
    }

    pub(crate) fn key(&self, thread_id: &str) -> CoreResult<String> {
        self.keyspace
            .thread_key(&self.scope, thread_id)
            .map_err(|e| CoreError::Session(format!("failed to build thread key for '{thread_id}': {e}")))
    }

    pub(crate) fn state_key(&self, thread_id: &str) -> CoreResult<String> {
        self.keyspace
            .thread_state_key(&self.scope, thread_id)
            .map_err(|e| CoreError::Session(format!("failed to build thread state key for '{thread_id}': {e}")))
    }

    fn list_prefix(&self) -> String {
        format!(
            "{}/",
            self.keyspace
                .threads_prefix(&self.scope)
                .trim_end_matches('/')
        )
    }

    fn ensure_thread_log_schema(log: &ThreadLog) -> CoreResult<()> {
        if log.schema_version != THREAD_SCHEMA_VERSION {
            return Err(CoreError::Schema(format!(
                "thread schema_version mismatch: expected {}, got {}",
                THREAD_SCHEMA_VERSION, log.schema_version
            )));
        }
        Ok(())
    }

    async fn load_thread_log_for_write(&self, key: &str) -> CoreResult<ThreadLog> {
        let log = if let Some(entry) = self.cache.get(key) {
            if entry.ts.elapsed().as_secs() < self.config.cache_ttl_secs {
                entry.log.clone()
            } else {
                drop(entry);
                if let Ok(v) = self.storage.get_json(key).await {
                    serde_json::from_value::<ThreadLog>(v)
                        .map_err(|e| CoreError::Session(format!("failed to parse thread log: {e}")))?
                } else {
                    ThreadLog::default()
                }
            }
        } else if let Ok(v) = self.storage.get_json(key).await {
            serde_json::from_value::<ThreadLog>(v)
                .map_err(|e| CoreError::Session(format!("failed to parse thread log: {e}")))?
        } else {
            ThreadLog::default()
        };
        Self::ensure_thread_log_schema(&log)?;
        Ok(log)
    }

    pub async fn append_step(&self, thread_id: &str, step: ThreadStep) -> CoreResult<()> {
        let key = self.key(thread_id)?;
        let mut log = self.load_thread_log_for_write(&key).await?;
        log.steps.push(step.clone());
        let step_count = log.steps.len();
        let val = serde_json::to_value(&log)
            .map_err(|e| CoreError::Session(format!("append_step('{}'): failed to serialize thread log: {}", thread_id, e)))?;
        self.storage.put_json(&key, &val).await?;
        self.cache.insert(
            key.clone(),
            CacheEntry {
                log,
                ts: Instant::now(),
            },
        );

        if let Err(e) = self
            .materialize_thread_state_incremental(thread_id, step_count, &step)
            .await
        {
            tracing::warn!(
                thread_id,
                step_count,
                error = %e,
                "view cache materialization failed (non-fatal)"
            );
        }
        Ok(())
    }

    pub async fn get_thread_state(&self, thread_id: &str) -> CoreResult<ThreadLogViewCache> {
        let key = self.state_key(thread_id)?;
        let v = self.storage.get_json(&key).await?;
        let s = serde_json::from_value::<ThreadLogViewCache>(v)
            .map_err(|e| CoreError::Session(format!("failed to parse thread state: {e}")))?;
        if s.thread_state_schema_version != THREAD_STATE_SCHEMA_VERSION {
            return Err(CoreError::Schema(format!(
                "thread_state schema_version mismatch: expected {}, got {}",
                THREAD_STATE_SCHEMA_VERSION, s.thread_state_schema_version
            )));
        }
        Ok(s)
    }

    pub(crate) async fn put_thread_state(&self, thread_id: &str, state: &ThreadLogViewCache) -> CoreResult<()> {
        self.write_thread_state(thread_id, state, ThreadLogViewCacheWriteMode::Merge)
            .await
    }

    async fn write_thread_state(
        &self,
        thread_id: &str,
        state: &ThreadLogViewCache,
        mode: ThreadLogViewCacheWriteMode,
    ) -> CoreResult<()> {
        if state.thread_state_schema_version != THREAD_STATE_SCHEMA_VERSION {
            return Err(CoreError::Schema(format!(
                "thread_state schema_version mismatch: expected {}, got {}",
                THREAD_STATE_SCHEMA_VERSION, state.thread_state_schema_version
            )));
        }
        if state.thread_id != thread_id {
            return Err(CoreError::Session(format!(
                "thread_state thread_id mismatch: expected {}, got {}",
                thread_id, state.thread_id
            )));
        }
        let next_state = match mode {
            ThreadLogViewCacheWriteMode::Replace => state.clone(),
            ThreadLogViewCacheWriteMode::Merge => {
                let mut merged = self
                    .get_thread_state(thread_id)
                    .await
                    .unwrap_or_else(|_| Self::new_thread_state(thread_id));
                if let Some(v) = state.suite_id.clone() {
                    merged.suite_id = Some(v);
                }
                if let Some(v) = state.agent_type.clone() {
                    merged.agent_type = Some(v);
                }
                if let Some(v) = state.current_phase.clone() {
                    merged.current_phase = Some(v);
                }
                merged.last_materialized_step_count = merged
                    .last_materialized_step_count
                    .max(state.last_materialized_step_count);
                merged.total_runtime_ms = merged.total_runtime_ms.max(state.total_runtime_ms);
                for (k, v) in state.items.iter() {
                    merged.items.insert(k.clone(), v.clone());
                }
                merged
            }
        };
        let mut merged_state = next_state;
        merged_state.thread_state_schema_version = THREAD_STATE_SCHEMA_VERSION;
        merged_state.thread_id = thread_id.to_string();
        let key = self.state_key(thread_id)?;
        let v = serde_json::to_value(&merged_state)
            .map_err(|e| CoreError::Session(format!("write_thread_state('{}'): failed to serialize: {}", thread_id, e)))?;
        self.storage.put_json(&key, &v).await
    }

    pub(crate) async fn put_thread_state_replace(
        &self,
        thread_id: &str,
        state: &ThreadLogViewCache,
    ) -> CoreResult<()> {
        self.write_thread_state(thread_id, state, ThreadLogViewCacheWriteMode::Replace)
            .await
    }

    pub async fn get_thread_events_from_log(&self, thread_id: &str) -> CoreResult<Vec<super::ThreadEvent>> {
        let log = self.get(thread_id).await?;
        Ok(super::build_thread_events_from_log(&log, self.config.max_events))
    }

    pub async fn get(&self, thread_id: &str) -> CoreResult<ThreadLog> {
        let key = self.key(thread_id)?;
        if let Some(entry) = self.cache.get(&key) {
            if entry.ts.elapsed().as_secs() < self.config.cache_ttl_secs {
                return Ok(entry.log.clone());
            }
        }
        let v = self
            .storage
            .get_json(&key)
            .await
            .map_err(|e| CoreError::Session(format!("get('{}'): {}", thread_id, e)))?;
        let log = serde_json::from_value::<ThreadLog>(v)
            .map_err(|e| CoreError::Session(format!("failed to parse thread log: {e}")))?;
        Self::ensure_thread_log_schema(&log)?;
        self.cache.insert(
            key.clone(),
            CacheEntry {
                log: log.clone(),
                ts: Instant::now(),
            },
        );
        Ok(log)
    }

    pub async fn list(&self) -> Vec<String> {
        let prefix = self.list_prefix();
        let mut out: Vec<String> = Vec::new();
        if let Ok(keys) = self.storage.list_prefix(&prefix).await {
            for k in keys {
                if let Some(name) = k
                    .strip_prefix(&prefix)
                    .and_then(|s| s.strip_suffix(".json"))
                {
                    if name.contains('.') {
                        continue;
                    }
                    out.push(name.to_string());
                }
            }
        }
        out.sort();
        out
    }

    pub async fn delete(&self, thread_id: &str) -> CoreResult<()> {
        let key = self.key(thread_id)?;
        let state_key = self.state_key(thread_id)?;
        let thread_prefix = format!(
            "{}/{}.",
            self.keyspace.threads_prefix(&self.scope).trim_end_matches('/'),
            thread_id
        );
        self.storage.delete_object(&key).await?;
        let _ = self.storage.delete_object(&state_key).await;
        if let Ok(keys) = self.storage.list_prefix(&thread_prefix).await {
            for k in keys {
                let _ = self.storage.delete_object(&k).await;
            }
        }
        self.cache.remove(&key);
        Ok(())
    }

    pub async fn set_title_if_absent(&self, thread_id: &str, title: &str) -> CoreResult<()> {
        let key = self.key(thread_id)?;
        let mut log = self.load_thread_log_for_write(&key).await?;
        if log.title.is_none() || log.title.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
            log.title = Some(title.to_string());
            let val = serde_json::to_value(&log)
                .map_err(|e| CoreError::Session(format!("set_title_if_absent('{}'): failed to serialize: {}", thread_id, e)))?;
            self.storage.put_json(&key, &val).await?;
            self.cache.insert(
                key.clone(),
                CacheEntry {
                    log,
                    ts: Instant::now(),
                },
            );
        }
        Ok(())
    }

    /// Ensure a "preflight" Phase step exists at the beginning of the thread.
    /// Returns `Some((step_idx, ts))` if the step was appended, `None` if one already existed.
    /// Append a step only if it's not a duplicate of the last step in the log.
    /// Deduplicates Interrupt and Complete steps by comparing payloads.
    pub async fn append_step_if_new(&self, thread_id: &str, step: ThreadStep) {
        let is_dup = match self
            .get(thread_id)
            .await
            .ok()
            .and_then(|l| l.steps.last().cloned())
        {
            Some(ThreadStep::Interrupt { prompt: p1, .. }) => {
                matches!(&step, ThreadStep::Interrupt { prompt: p2, .. } if p1 == *p2)
            }
            Some(ThreadStep::Complete {
                kind: k1,
                payload: pl1,
                display: d1,
                ..
            }) => matches!(
                &step,
                ThreadStep::Complete { kind: k2, payload: pl2, display: d2, .. }
                    if k1 == *k2 && pl1 == *pl2 && d1 == *d2
            ),
            _ => false,
        };
        if !is_dup {
            let _ = self.append_step(thread_id, step).await;
        }
    }

    pub async fn ensure_preflight_phase_step(
        &self,
        thread_id: &str,
        agent: &str,
        suite_id: Option<&str>,
        initial_phase: &str,
    ) -> CoreResult<Option<(usize, String)>> {
        let log = self.get(thread_id).await.unwrap_or_default();
        let already_has_phase = log.steps.iter().any(|s| matches!(s, ThreadStep::Phase { .. }));
        if already_has_phase {
            return Ok(None);
        }
        let step_idx = log.steps.len();
        let ts = chrono::Utc::now().to_rfc3339();
        let mut detail = serde_json::Map::new();
        detail.insert("phase".to_string(), serde_json::json!(initial_phase));
        if let Some(s) = suite_id {
            if !s.trim().is_empty() {
                detail.insert("suite_id".to_string(), serde_json::json!(s));
            }
        }
        if !agent.trim().is_empty() {
            detail.insert("agent".to_string(), serde_json::json!(agent));
        }
        let _ = self
            .append_step(
                thread_id,
                ThreadStep::Phase {
                    phase: initial_phase.to_string(),
                    from_phase: None,
                    reason_code: Some("preflight_start".to_string()),
                    reason_detail: Some(serde_json::Value::Object(detail)),
                    observation: super::Observation::ok(),
                    ts: ts.clone(),
                    agent: agent.to_string(),
                },
            )
            .await;
        Ok(Some((step_idx, ts)))
    }

    pub async fn lock_title(&self, thread_id: &str, title: &str) -> CoreResult<()> {
        let key = self.key(thread_id)?;
        let mut log = self.load_thread_log_for_write(&key).await?;
        if !log.title_locked {
            log.title = Some(title.to_string());
            log.title_locked = true;
            let val = serde_json::to_value(&log)
                .map_err(|e| CoreError::Session(format!("lock_title('{}'): failed to serialize: {}", thread_id, e)))?;
            self.storage.put_json(&key, &val).await?;
            self.cache.insert(
                key.clone(),
                CacheEntry {
                    log,
                    ts: Instant::now(),
                },
            );
        }
        Ok(())
    }
}
