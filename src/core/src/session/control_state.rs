use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::error::{CoreError, CoreResult};
use crate::keyspace::Keyspace;
use crate::scope::RequestScope;
use crate::storage::{ConditionalWriteStatus, StorageAdapter};

use super::{
    session_write_lock, ControlStateEnvelope, LoadState, VersionedValue,
    CONTROL_STATE_ENVELOPE_SCHEMA_VERSION,
};

#[derive(Clone)]
pub struct ControlStateStore {
    storage: Arc<dyn StorageAdapter>,
    scope: RequestScope,
    keyspace: Arc<dyn Keyspace>,
}

impl std::fmt::Debug for ControlStateStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ControlStateStore { .. }")
    }
}

impl ControlStateStore {
    pub fn new(
        storage: Arc<dyn StorageAdapter>,
        scope: RequestScope,
        keyspace: Arc<dyn Keyspace>,
    ) -> Self {
        Self {
            storage,
            scope,
            keyspace,
        }
    }

    pub fn storage(&self) -> &Arc<dyn StorageAdapter> {
        &self.storage
    }

    pub fn scope(&self) -> &RequestScope {
        &self.scope
    }

    pub fn keyspace(&self) -> &Arc<dyn Keyspace> {
        &self.keyspace
    }

    fn key(&self, thread_id: &str) -> CoreResult<String> {
        self.keyspace
            .control_state_key(&self.scope, thread_id)
            .map_err(|e| {
                CoreError::Session(format!(
                    "failed to build control state key for '{thread_id}': {e}"
                ))
            })
    }

    fn decode(raw: &Value, expected_suite_id: &str) -> CoreResult<Option<Value>> {
        let env = serde_json::from_value::<ControlStateEnvelope>(raw.clone()).map_err(|e| {
            CoreError::Session(format!(
                "control state envelope deserialization failed (suite='{expected_suite_id}'): {e}"
            ))
        })?;
        if env.schema_version == CONTROL_STATE_ENVELOPE_SCHEMA_VERSION
            && env.suite_id.trim() == expected_suite_id.trim()
        {
            Ok(Some(env.payload))
        } else {
            Ok(None)
        }
    }

    fn encode(suite_id: &str, payload: Value) -> CoreResult<Value> {
        serde_json::to_value(ControlStateEnvelope {
            schema_version: CONTROL_STATE_ENVELOPE_SCHEMA_VERSION,
            suite_id: suite_id.trim().to_string(),
            payload,
        })
        .map_err(|e| {
            CoreError::Session(format!("encode_control_state(suite_id='{suite_id}'): {e}"))
        })
    }

    async fn load_raw(&self, thread_id: &str) -> CoreResult<LoadState<Value>> {
        let key = self.key(thread_id)?;
        let etag = self.storage.head_etag(&key).await?;
        let Some(etag) = etag else {
            return Ok(LoadState::Missing);
        };
        let value = self.storage.get_json(&key).await?;
        Ok(LoadState::Loaded(VersionedValue {
            etag: Some(etag),
            value,
        }))
    }

    async fn save_raw(&self, thread_id: &str, value: &Value) -> CoreResult<()> {
        let key = self.key(thread_id)?;
        self.storage.put_json(&key, value).await
    }

    async fn save_raw_if_etag_matches(
        &self,
        thread_id: &str,
        value: &Value,
        expected_etag: Option<&str>,
    ) -> CoreResult<()> {
        let key = self.key(thread_id)?;
        let expected = expected_etag.map(str::to_string);
        match self
            .storage
            .put_json_if_etag_matches(&key, value, expected_etag)
            .await?
        {
            ConditionalWriteStatus::Written => Ok(()),
            ConditionalWriteStatus::Conflict { current_etag } => Err(CoreError::Session(format!(
                "control_state('{}'): write conflict detected (expected_etag={:?}, current_etag={:?})",
                thread_id, expected, current_etag
            ))),
        }
    }

    pub async fn load<T: DeserializeOwned>(
        &self,
        thread_id: &str,
        suite_id: &str,
    ) -> CoreResult<Option<T>> {
        let raw = match self.load_raw(thread_id).await? {
            LoadState::Missing => return Ok(None),
            LoadState::Loaded(raw) => raw.value,
        };
        let Some(payload) = Self::decode(&raw, suite_id)? else {
            return Ok(None);
        };
        let parsed = serde_json::from_value::<T>(payload).map_err(|e| {
            CoreError::Session(format!("failed to parse control state payload: {e}"))
        })?;
        Ok(Some(parsed))
    }

    pub async fn save<T: Serialize>(
        &self,
        thread_id: &str,
        suite_id: &str,
        state: &T,
    ) -> CoreResult<()> {
        let payload = serde_json::to_value(state).map_err(|e| {
            CoreError::Session(format!("failed to serialize control state payload: {e}"))
        })?;
        let envelope = Self::encode(suite_id, payload)?;
        self.save_raw(thread_id, &envelope).await
    }

    pub async fn mutate<T: Serialize + DeserializeOwned>(
        &self,
        thread_id: &str,
        suite_id: &str,
        mutate: impl FnOnce(Option<T>) -> CoreResult<T>,
    ) -> CoreResult<T> {
        let key = self.key(thread_id)?;
        let lock = session_write_lock(&key);
        let _guard = lock.lock().await;
        let loaded = self.load_raw(thread_id).await?;
        let expected_etag = match &loaded {
            LoadState::Missing => None,
            LoadState::Loaded(raw) => raw.etag.clone(),
        };
        let current = match loaded {
            LoadState::Missing => None,
            LoadState::Loaded(raw) => match Self::decode(&raw.value, suite_id)? {
                Some(payload) => Some(serde_json::from_value::<T>(payload).map_err(|e| {
                    CoreError::Session(format!("failed to parse control state payload: {e}"))
                })?),
                None => None,
            },
        };
        let next = mutate(current)?;
        let payload = serde_json::to_value(&next).map_err(|e| {
            CoreError::Session(format!("failed to serialize control state payload: {e}"))
        })?;
        let envelope = Self::encode(suite_id, payload)?;
        self.save_raw_if_etag_matches(thread_id, &envelope, expected_etag.as_deref())
            .await?;
        Ok(next)
    }

    pub async fn load_suite_id(&self, thread_id: &str) -> CoreResult<Option<String>> {
        let raw = match self.load_raw(thread_id).await? {
            LoadState::Missing => return Ok(None),
            LoadState::Loaded(raw) => raw.value,
        };
        let env = serde_json::from_value::<ControlStateEnvelope>(raw).ok();
        Ok(env.map(|e| e.suite_id).filter(|s| !s.trim().is_empty()))
    }

    pub async fn delete(&self, thread_id: &str) -> CoreResult<()> {
        let key = self.key(thread_id)?;
        let _ = self.storage.delete_object(&key).await;
        Ok(())
    }
}
