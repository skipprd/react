use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde_json::Value;

use react_core::error::CoreError;
use react_core::storage::StorageAdapter;

#[derive(Clone, Default)]
pub struct InMemoryStorageAdapter {
    inner: Arc<RwLock<HashMap<String, StoredObject>>>,
}

#[derive(Clone, Debug)]
struct StoredObject {
    bytes: Vec<u8>,
    etag: String,
}

impl InMemoryStorageAdapter {
    fn next_etag(bytes: &[u8]) -> String {
        format!("mem-etag-{}", bytes.len())
    }
}

#[async_trait]
impl StorageAdapter for InMemoryStorageAdapter {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        let bytes = self.get_bytes(key).await?;
        serde_json::from_slice::<Value>(&bytes).map_err(|e| {
            CoreError::Storage(format!(
                "get_json('{}'): failed to deserialize JSON: {}",
                key, e
            ))
        })
    }

    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec(value).map_err(|e| {
            CoreError::Storage(format!(
                "put_json('{}'): failed to serialize JSON: {}",
                key, e
            ))
        })?;
        self.put_bytes(key, &bytes, "application/json").await
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        let g = self
            .inner
            .read()
            .map_err(|_| CoreError::Storage(format!("get_bytes('{}'): storage lock poisoned", key)))?;
        g.get(key)
            .map(|o| o.bytes.clone())
            .ok_or_else(|| CoreError::Storage(format!("get_bytes('{}'): not found", key)))
    }

    async fn put_bytes(
        &self,
        key: &str,
        bytes: &[u8],
        _content_type: &str,
    ) -> Result<(), CoreError> {
        let mut g = self.inner.write().map_err(|_| {
            CoreError::Storage(format!("put_bytes('{}'): storage lock poisoned", key))
        })?;
        g.insert(
            key.to_string(),
            StoredObject {
                bytes: bytes.to_vec(),
                etag: Self::next_etag(bytes),
            },
        );
        Ok(())
    }

    async fn delete_object(&self, key: &str) -> Result<(), CoreError> {
        let mut g = self.inner.write().map_err(|_| {
            CoreError::Storage(format!("delete_object('{}'): storage lock poisoned", key))
        })?;
        g.remove(key);
        Ok(())
    }

    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        let g = self
            .inner
            .read()
            .map_err(|_| CoreError::Storage(format!("head_etag('{}'): storage lock poisoned", key)))?;
        Ok(g.get(key).map(|o| o.etag.clone()))
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError> {
        let g = self.inner.read().map_err(|_| {
            CoreError::Storage(format!("list_prefix('{}'): storage lock poisoned", prefix))
        })?;
        let mut out: Vec<String> = g.keys().filter(|k| k.starts_with(prefix)).cloned().collect();
        out.sort();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roundtrip_json() {
        let store = InMemoryStorageAdapter::default();
        let val = serde_json::json!({"hello": "world"});
        store.put_json("k", &val).await.unwrap();
        assert_eq!(store.get_json("k").await.unwrap(), val);
    }

    #[tokio::test]
    async fn roundtrip_bytes() {
        let store = InMemoryStorageAdapter::default();
        store.put_bytes("b", b"data", "text/plain").await.unwrap();
        assert_eq!(store.get_bytes("b").await.unwrap(), b"data");
    }

    #[tokio::test]
    async fn list_prefix_filters() {
        let store = InMemoryStorageAdapter::default();
        store.put_bytes("a/1", b"", "x").await.unwrap();
        store.put_bytes("a/2", b"", "x").await.unwrap();
        store.put_bytes("b/1", b"", "x").await.unwrap();
        let keys = store.list_prefix("a/").await.unwrap();
        assert_eq!(keys, vec!["a/1", "a/2"]);
    }

    #[tokio::test]
    async fn delete_and_head() {
        let store = InMemoryStorageAdapter::default();
        store.put_bytes("x", b"123", "x").await.unwrap();
        assert!(store.head_etag("x").await.unwrap().is_some());
        store.delete_object("x").await.unwrap();
        assert!(store.head_etag("x").await.unwrap().is_none());
    }
}
