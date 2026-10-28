use std::sync::Arc;

use crate::error::{CoreError, CoreResult};
use crate::storage::{
    retry_delete_object, retry_get_json, retry_head_etag, retry_list_prefix,
    retry_put_json_if_etag_matches, ConditionalWriteStatus,
};

use super::{
    session_write_lock, CacheEntry, LoadState, Observation, ThreadLog, ThreadResult, ThreadStep,
    ThreadStore, ThreadStoreConfig, VersionedValue, THREAD_SCHEMA_VERSION,
};

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
            .map_err(|e| {
                CoreError::Session(format!("failed to build thread key for '{thread_id}': {e}"))
            })
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

    async fn load_thread_log_for_write(&self, key: &str) -> CoreResult<LoadState<ThreadLog>> {
        let current_etag = retry_head_etag(self.storage.as_ref(), key).await?;
        let Some(etag) = current_etag else {
            return Ok(LoadState::Missing);
        };
        let cached = self
            .cache
            .get(key)
            .and_then(|entry| (entry.etag == etag).then(|| entry.log.clone()));
        let log = if let Some(log) = cached {
            log
        } else {
            let v = retry_get_json(self.storage.as_ref(), key).await?;
            let log = serde_json::from_value::<ThreadLog>(v)
                .map_err(|e| CoreError::Session(format!("failed to parse thread log: {e}")))?;
            self.remember_log(key, log.clone(), etag.clone());
            log
        };
        Self::ensure_thread_log_schema(&log)?;
        Ok(LoadState::Loaded(VersionedValue {
            etag: Some(etag),
            value: log,
        }))
    }

    async fn save_thread_log_if_etag_matches(
        &self,
        thread_id: &str,
        key: &str,
        log: &ThreadLog,
        expected_etag: Option<&str>,
    ) -> CoreResult<String> {
        let expected = expected_etag.map(str::to_string);
        let val = serde_json::to_value(log).map_err(|e| {
            CoreError::Session(format!(
                "thread_log('{thread_id}'): failed to serialize thread log: {e}"
            ))
        })?;
        match retry_put_json_if_etag_matches(self.storage.as_ref(), key, &val, expected_etag)
            .await?
        {
            ConditionalWriteStatus::Written { etag } => Ok(etag),
            ConditionalWriteStatus::Conflict { current_etag } => Err(CoreError::AuditConflict(
                format!(
                    "thread_log('{thread_id}'): write conflict detected (expected_etag={expected:?}, current_etag={current_etag:?})"
                ),
            )),
        }
    }

    fn remember_log(&self, key: &str, log: ThreadLog, etag: String) {
        self.cache.insert(key.to_string(), CacheEntry { log, etag });
    }

    pub async fn append_step(&self, thread_id: &str, step: ThreadStep) -> CoreResult<()> {
        let key = self.key(thread_id)?;
        let lock = session_write_lock(&key);
        let _guard = lock.lock().await;
        for _attempt in 0..=crate::budget::AUDIT_MAX_CONFLICT_RETRIES {
            match self.append_step_locked(&key, thread_id, step.clone()).await {
                Ok(()) => return Ok(()),
                Err(CoreError::AuditConflict(_)) => {
                    self.cache.remove(&key);
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err(CoreError::AuditUnavailable {
            thread: thread_id.to_string(),
            retries: crate::budget::AUDIT_MAX_CONFLICT_RETRIES,
        })
    }

    async fn append_step_locked(
        &self,
        key: &str,
        thread_id: &str,
        step: ThreadStep,
    ) -> CoreResult<()> {
        let loaded = self.load_thread_log_for_write(key).await?;
        let expected_etag = match &loaded {
            LoadState::Missing => None,
            LoadState::Loaded(v) => v.etag.clone(),
        };
        let mut log = match loaded {
            LoadState::Missing => ThreadLog::default(),
            LoadState::Loaded(v) => v.value,
        };
        if let ThreadStep::Complete {
            kind,
            payload,
            display,
            ..
        } = &step
        {
            log.result = Some(ThreadResult {
                kind: kind.clone(),
                payload: payload.clone(),
                display: display.clone(),
            });
        }
        log.steps.push(step);
        let etag = self
            .save_thread_log_if_etag_matches(thread_id, key, &log, expected_etag.as_deref())
            .await?;
        self.remember_log(key, log, etag);
        Ok(())
    }

    pub async fn append_step_if_new(&self, thread_id: &str, step: ThreadStep) -> CoreResult<()> {
        let is_dup = match &step {
            ThreadStep::Complete { kind, payload, .. } => self
                .get(thread_id)
                .await
                .ok()
                .map(|log| {
                    log.steps.iter().rev().any(|s| {
                        matches!(s, ThreadStep::Complete { kind: k, payload: p, .. } if k == kind && p == payload)
                    })
                })
                .unwrap_or(false),
            ThreadStep::Interrupt { kind, prompt, .. } => self
                .get(thread_id)
                .await
                .ok()
                .map(|log| {
                    log.steps.iter().rev().any(|s| {
                        matches!(s, ThreadStep::Interrupt { kind: k, prompt: p, .. } if k == kind && p == prompt)
                    })
                })
                .unwrap_or(false),
            _ => false,
        };
        if !is_dup {
            self.append_step(thread_id, step).await?;
        }
        Ok(())
    }

    pub async fn ensure_preflight_phase_step(
        &self,
        thread_id: &str,
        agent: &str,
        suite_id: Option<&str>,
        initial_phase: &str,
    ) -> CoreResult<Option<(usize, String)>> {
        let key = self.key(thread_id)?;
        let lock = session_write_lock(&key);
        let _guard = lock.lock().await;
        let loaded = self.load_thread_log_for_write(&key).await?;
        let expected_etag = match &loaded {
            LoadState::Missing => None,
            LoadState::Loaded(v) => v.etag.clone(),
        };
        let mut log = match loaded {
            LoadState::Missing => ThreadLog::default(),
            LoadState::Loaded(v) => v.value,
        };
        for (idx, step) in log.steps.iter().enumerate() {
            if let ThreadStep::Phase { phase, .. } = step {
                return Ok(Some((idx, phase.clone())));
            }
        }
        let reason_detail = suite_id.map(|sid| serde_json::json!({"suite_id": sid}));
        log.steps.push(ThreadStep::Phase {
            phase: initial_phase.to_string(),
            from_phase: None,
            reason_code: Some("thread_started".to_string()),
            reason_detail,
            observation: Observation::ok(),
            ts: chrono::Utc::now().to_rfc3339(),
            agent: agent.to_string(),
        });
        let idx = log.steps.len().saturating_sub(1);
        let etag = self
            .save_thread_log_if_etag_matches(thread_id, &key, &log, expected_etag.as_deref())
            .await?;
        self.remember_log(&key, log, etag);
        Ok(Some((idx, initial_phase.to_string())))
    }

    pub async fn get(&self, thread_id: &str) -> CoreResult<ThreadLog> {
        let key = self.key(thread_id)?;
        let current_etag = retry_head_etag(self.storage.as_ref(), &key).await?;
        if let Some(etag) = current_etag.as_ref() {
            if let Some(entry) = self.cache.get(&key) {
                if entry.etag == *etag {
                    return Ok(entry.log.clone());
                }
            }
        }
        let v = self.storage.as_ref();
        let v = retry_get_json(v, &key)
            .await
            .map_err(|e| CoreError::Session(format!("get('{thread_id}'): {e}")))?;
        let log = serde_json::from_value::<ThreadLog>(v)
            .map_err(|e| CoreError::Session(format!("failed to parse thread log: {e}")))?;
        Self::ensure_thread_log_schema(&log)?;
        if let Some(etag) = current_etag {
            self.remember_log(&key, log.clone(), etag);
        }
        Ok(log)
    }

    pub async fn list(&self) -> Vec<String> {
        let prefix = self.list_prefix();
        let mut out: Vec<String> = Vec::new();
        if let Ok(keys) = retry_list_prefix(self.storage.as_ref(), &prefix).await {
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
        let thread_prefix = format!(
            "{}/{}.",
            self.keyspace
                .threads_prefix(&self.scope)
                .trim_end_matches('/'),
            thread_id
        );
        retry_delete_object(self.storage.as_ref(), &key).await?;
        let keys = retry_list_prefix(self.storage.as_ref(), &thread_prefix).await?;
        for k in keys {
            retry_delete_object(self.storage.as_ref(), &k).await?;
        }
        self.cache.remove(&key);
        Ok(())
    }

    pub async fn set_title_if_absent(&self, thread_id: &str, title: &str) -> CoreResult<()> {
        let key = self.key(thread_id)?;
        let lock = session_write_lock(&key);
        let _guard = lock.lock().await;
        let loaded = self.load_thread_log_for_write(&key).await?;
        let expected_etag = match &loaded {
            LoadState::Missing => None,
            LoadState::Loaded(v) => v.etag.clone(),
        };
        let mut log = match loaded {
            LoadState::Missing => ThreadLog::default(),
            LoadState::Loaded(v) => v.value,
        };
        if log.title.is_none() || log.title.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
            log.title = Some(title.to_string());
            let etag = self
                .save_thread_log_if_etag_matches(thread_id, &key, &log, expected_etag.as_deref())
                .await?;
            self.remember_log(&key, log, etag);
        }
        Ok(())
    }

    pub async fn lock_title(&self, thread_id: &str, title: &str) -> CoreResult<()> {
        let key = self.key(thread_id)?;
        let lock = session_write_lock(&key);
        let _guard = lock.lock().await;
        let loaded = self.load_thread_log_for_write(&key).await?;
        let expected_etag = match &loaded {
            LoadState::Missing => None,
            LoadState::Loaded(v) => v.etag.clone(),
        };
        let mut log = match loaded {
            LoadState::Missing => ThreadLog::default(),
            LoadState::Loaded(v) => v.value,
        };
        if !log.title_locked {
            log.title = Some(title.to_string());
            log.title_locked = true;
            let etag = self
                .save_thread_log_if_etag_matches(thread_id, &key, &log, expected_etag.as_deref())
                .await?;
            self.remember_log(&key, log, etag);
        }
        Ok(())
    }
}
