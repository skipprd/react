use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde_json::Value;

use crate::error::CoreError;
use crate::storage::{ConditionalWriteStatus, StorageAdapter};

/// Minimal in-memory storage for core-internal tests only.
/// External crates should use `react-module-storage-memory` instead.
#[derive(Clone, Default)]
pub(crate) struct InMemoryStorageAdapter {
    inner: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl InMemoryStorageAdapter {
    fn etag(bytes: &[u8]) -> String {
        let mut hash: u64 = 1469598103934665603;
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
        format!("mem-etag-{:016x}-{}", hash, bytes.len())
    }
}

#[async_trait]
impl StorageAdapter for InMemoryStorageAdapter {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        let bytes = self.get_bytes(key).await?;
        serde_json::from_slice(&bytes)
            .map_err(|e| CoreError::Storage(format!("get_json('{key}'): {e}")))
    }
    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| CoreError::Storage(format!("put_json('{key}'): {e}")))?;
        self.put_bytes(key, &bytes, "application/json").await
    }
    async fn put_json_if_etag_matches(
        &self,
        key: &str,
        value: &Value,
        expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError> {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| CoreError::Storage(format!("put_json_if_etag_matches('{key}'): {e}")))?;
        let mut g = self
            .inner
            .write()
            .map_err(|_| CoreError::Storage("lock poisoned".into()))?;
        let current_etag = g.get(key).map(|b| Self::etag(b));
        let expected = expected_etag.map(|s| s.to_string());
        if current_etag != expected {
            return Ok(ConditionalWriteStatus::Conflict { current_etag });
        }
        g.insert(key.to_string(), bytes);
        Ok(ConditionalWriteStatus::Written)
    }
    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        let g = self
            .inner
            .read()
            .map_err(|_| CoreError::Storage("lock poisoned".into()))?;
        g.get(key)
            .cloned()
            .ok_or_else(|| CoreError::Storage(format!("not found: {key}")))
    }
    async fn put_bytes(&self, key: &str, bytes: &[u8], _ct: &str) -> Result<(), CoreError> {
        let mut g = self
            .inner
            .write()
            .map_err(|_| CoreError::Storage("lock poisoned".into()))?;
        g.insert(key.to_string(), bytes.to_vec());
        Ok(())
    }
    async fn delete_object(&self, key: &str) -> Result<(), CoreError> {
        let mut g = self
            .inner
            .write()
            .map_err(|_| CoreError::Storage("lock poisoned".into()))?;
        g.remove(key);
        Ok(())
    }
    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        let g = self
            .inner
            .read()
            .map_err(|_| CoreError::Storage("lock poisoned".into()))?;
        Ok(g.get(key).map(|b| Self::etag(b)))
    }
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError> {
        let g = self
            .inner
            .read()
            .map_err(|_| CoreError::Storage("lock poisoned".into()))?;
        let mut out: Vec<String> = g
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        out.sort();
        Ok(out)
    }
}
