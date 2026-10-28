use crate::error::CoreError;
use crate::retry::{retry_classified, Classify};
use crate::text::truncate_chars_ellipsis;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionalWriteStatus {
    Written { etag: String },
    Conflict { current_etag: Option<String> },
}

/// Exponential backoff for transient S3/client errors.
pub const SAFE_STORAGE_RETRY_DELAYS_MS: [u64; 6] = [100, 200, 400, 800, 1_600, 3_200];

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StorageError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("auth expired: {0}")]
    AuthExpired(String),
    #[error("throttle: {0}")]
    Throttle(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("{0}")]
    Other(String),
}

impl StorageError {
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }

    pub fn not_found(key: impl Into<String>) -> Self {
        Self::NotFound(key.into())
    }

    pub fn retry(&self) -> bool {
        matches!(
            self,
            Self::AuthExpired(_) | Self::Throttle(_) | Self::Timeout(_)
        )
    }

    pub fn from_io(op: &str, key: &str, err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound(key.to_string()),
            std::io::ErrorKind::TimedOut => Self::Timeout(format!("{op}('{key}'): {err}")),
            _ => Self::Other(format!("{op}('{key}'): {err}")),
        }
    }
}

impl Classify for StorageError {
    fn retry(&self) -> bool {
        self.retry()
    }
}

#[derive(Clone, Debug)]
enum CacheEntry {
    Bytes {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
    Missing,
}

#[async_trait]
pub trait StorageAdapter: Send + Sync {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError>;
    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError>;
    async fn put_json_if_etag_matches(
        &self,
        key: &str,
        value: &Value,
        expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError>;

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError>;
    async fn put_bytes(&self, key: &str, bytes: &[u8], content_type: &str)
        -> Result<(), CoreError>;

    async fn delete_object(&self, key: &str) -> Result<(), CoreError>;
    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError>;

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError>;
}

#[derive(Clone)]
pub struct CachingStorageAdapter {
    inner: Arc<dyn StorageAdapter>,
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
}

impl CachingStorageAdapter {
    pub fn new(inner: Arc<dyn StorageAdapter>) -> Self {
        Self {
            inner,
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn read_entry(&self, key: &str) -> Result<Option<CacheEntry>, CoreError> {
        let entries = self.entries.read().map_err(|_| {
            CoreError::from(StorageError::other(format!(
                "storage cache read lock poisoned for '{key}'"
            )))
        })?;
        Ok(entries.get(key).cloned())
    }

    fn write_entry(&self, key: &str, entry: CacheEntry) -> Result<(), CoreError> {
        let mut entries = self.entries.write().map_err(|_| {
            CoreError::from(StorageError::other(format!(
                "storage cache write lock poisoned for '{key}'"
            )))
        })?;
        entries.insert(key.to_string(), entry);
        Ok(())
    }

    fn invalidate_entry(&self, key: &str) -> Result<(), CoreError> {
        let mut entries = self.entries.write().map_err(|_| {
            CoreError::from(StorageError::other(format!(
                "storage cache write lock poisoned for '{key}'"
            )))
        })?;
        entries.remove(key);
        Ok(())
    }

    fn cached_missing_error(key: &str) -> CoreError {
        CoreError::from(StorageError::not_found(key))
    }

    async fn remember_bytes(
        &self,
        key: &str,
        bytes: Vec<u8>,
        etag: Option<String>,
    ) -> Result<(), CoreError> {
        self.write_entry(key, CacheEntry::Bytes { bytes, etag })
    }
}

pub fn cached(inner: Arc<dyn StorageAdapter>) -> Arc<dyn StorageAdapter> {
    Arc::new(CachingStorageAdapter::new(inner))
}

#[async_trait]
impl StorageAdapter for CachingStorageAdapter {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        let bytes = self.get_bytes(key).await?;
        serde_json::from_slice::<Value>(&bytes).map_err(|e| {
            CoreError::from(StorageError::other(format!(
                "get_json('{key}'): failed to deserialize JSON: {e}"
            )))
        })
    }

    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec(value).map_err(|e| {
            CoreError::from(StorageError::other(format!(
                "put_json('{key}'): failed to serialize JSON: {e}"
            )))
        })?;
        self.inner
            .put_bytes(key, &bytes, "application/json")
            .await?;
        self.remember_bytes(key, bytes, None).await
    }

    async fn put_json_if_etag_matches(
        &self,
        key: &str,
        value: &Value,
        expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError> {
        let bytes = serde_json::to_vec(value).map_err(|e| {
            CoreError::from(StorageError::other(format!(
                "put_json_if_etag_matches('{key}'): failed to serialize JSON: {e}"
            )))
        })?;
        let status = self
            .inner
            .put_json_if_etag_matches(key, value, expected_etag)
            .await?;
        match status {
            ConditionalWriteStatus::Written { etag } => {
                self.remember_bytes(key, bytes, Some(etag.clone())).await?;
                Ok(ConditionalWriteStatus::Written { etag })
            }
            ConditionalWriteStatus::Conflict { current_etag } => {
                self.invalidate_entry(key)?;
                Ok(ConditionalWriteStatus::Conflict { current_etag })
            }
        }
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        match self.read_entry(key)? {
            Some(CacheEntry::Bytes {
                bytes,
                etag: Some(etag),
            }) => {
                if self.inner.head_etag(key).await?.as_deref() == Some(etag.as_str()) {
                    return Ok(bytes);
                }
            }
            Some(CacheEntry::Bytes { bytes, etag: None }) => return Ok(bytes),
            Some(CacheEntry::Missing) => return Err(Self::cached_missing_error(key)),
            None => {}
        }

        let current_etag = self.inner.head_etag(key).await?;
        match self.inner.get_bytes(key).await {
            Ok(bytes) => {
                self.remember_bytes(key, bytes.clone(), current_etag)
                    .await?;
                Ok(bytes)
            }
            Err(err) if is_storage_not_found_error(&err) => {
                self.write_entry(key, CacheEntry::Missing)?;
                Err(err)
            }
            Err(err) => Err(err),
        }
    }

    async fn put_bytes(
        &self,
        key: &str,
        bytes: &[u8],
        content_type: &str,
    ) -> Result<(), CoreError> {
        self.inner.put_bytes(key, bytes, content_type).await?;
        self.remember_bytes(key, bytes.to_vec(), None).await
    }

    async fn delete_object(&self, key: &str) -> Result<(), CoreError> {
        self.inner.delete_object(key).await?;
        self.write_entry(key, CacheEntry::Missing)
    }

    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        if matches!(self.read_entry(key)?, Some(CacheEntry::Missing)) {
            return Ok(None);
        }
        self.inner.head_etag(key).await
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError> {
        self.inner.list_prefix(prefix).await
    }
}

pub fn classify_storage_error(err: &CoreError) -> Option<&StorageError> {
    match err {
        CoreError::Storage(e) => Some(e),
        _ => None,
    }
}

pub fn is_storage_not_found_error(err: &CoreError) -> bool {
    matches!(err, CoreError::Storage(StorageError::NotFound(_)))
}

pub fn should_retry_storage_error(err: &CoreError) -> bool {
    matches!(err, CoreError::Storage(e) if e.retry())
}

fn summarize_storage_retry_error(err: &CoreError) -> String {
    truncate_chars_ellipsis(&err.to_string(), 240)
}

pub async fn retry_storage_call<T, Fut, F>(
    operation: &'static str,
    target_kind: &'static str,
    target: &str,
    call: F,
) -> Result<T, CoreError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, CoreError>>,
{
    retry_classified(&SAFE_STORAGE_RETRY_DELAYS_MS, call, |retry_count, err| {
        tracing::warn!(
            operation,
            target_kind,
            target,
            retry_count,
            max_retries = SAFE_STORAGE_RETRY_DELAYS_MS.len(),
            error = %summarize_storage_retry_error(err),
            "storage operation failed; retrying"
        );
        tracing::debug!(
            operation,
            target_kind,
            target,
            retry_count,
            full_error = %err,
            "storage retry raw error detail"
        );
    })
    .await
}

pub async fn retry_get_json(storage: &dyn StorageAdapter, key: &str) -> Result<Value, CoreError> {
    retry_storage_call("get_json", "key", key, || storage.get_json(key)).await
}

pub async fn retry_put_json(
    storage: &dyn StorageAdapter,
    key: &str,
    value: &Value,
) -> Result<(), CoreError> {
    retry_storage_call("put_json", "key", key, || storage.put_json(key, value)).await
}

pub async fn retry_put_json_if_etag_matches(
    storage: &dyn StorageAdapter,
    key: &str,
    value: &Value,
    expected_etag: Option<&str>,
) -> Result<ConditionalWriteStatus, CoreError> {
    retry_storage_call("put_json_if_etag_matches", "key", key, || {
        storage.put_json_if_etag_matches(key, value, expected_etag)
    })
    .await
}

pub async fn retry_get_bytes(
    storage: &dyn StorageAdapter,
    key: &str,
) -> Result<Vec<u8>, CoreError> {
    retry_storage_call("get_bytes", "key", key, || storage.get_bytes(key)).await
}

pub async fn retry_put_bytes(
    storage: &dyn StorageAdapter,
    key: &str,
    bytes: &[u8],
    content_type: &str,
) -> Result<(), CoreError> {
    retry_storage_call("put_bytes", "key", key, || {
        storage.put_bytes(key, bytes, content_type)
    })
    .await
}

pub async fn retry_delete_object(storage: &dyn StorageAdapter, key: &str) -> Result<(), CoreError> {
    retry_storage_call("delete_object", "key", key, || storage.delete_object(key)).await
}

pub async fn retry_head_etag(
    storage: &dyn StorageAdapter,
    key: &str,
) -> Result<Option<String>, CoreError> {
    retry_storage_call("head_etag", "key", key, || storage.head_etag(key)).await
}

pub async fn retry_list_prefix(
    storage: &dyn StorageAdapter,
    prefix: &str,
) -> Result<Vec<String>, CoreError> {
    retry_storage_call("list_prefix", "prefix", prefix, || {
        storage.list_prefix(prefix)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::{
        is_storage_not_found_error, retry_storage_call, should_retry_storage_error,
        summarize_storage_retry_error, CachingStorageAdapter, ConditionalWriteStatus,
        StorageAdapter, StorageError,
    };
    use crate::error::CoreError;
    use async_trait::async_trait;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct CountingStorageAdapter {
        bytes: Mutex<HashMap<String, Vec<u8>>>,
        get_bytes_calls: AtomicUsize,
        put_bytes_calls: AtomicUsize,
        delete_calls: AtomicUsize,
        head_etag_calls: AtomicUsize,
        force_conflict: AtomicBool,
    }

    impl CountingStorageAdapter {
        fn etag_for(bytes: &[u8]) -> String {
            let mut hash: u64 = 1469598103934665603;
            for b in bytes {
                hash ^= *b as u64;
                hash = hash.wrapping_mul(1099511628211);
            }
            format!("count-etag-{:016x}-{}", hash, bytes.len())
        }

        fn seed_bytes(&self, key: &str, bytes: &[u8]) {
            self.bytes
                .lock()
                .expect("bytes lock")
                .insert(key.to_string(), bytes.to_vec());
        }

        fn get_bytes_calls(&self) -> usize {
            self.get_bytes_calls.load(Ordering::SeqCst)
        }

        fn put_bytes_calls(&self) -> usize {
            self.put_bytes_calls.load(Ordering::SeqCst)
        }

        fn delete_calls(&self) -> usize {
            self.delete_calls.load(Ordering::SeqCst)
        }

        fn head_etag_calls(&self) -> usize {
            self.head_etag_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl StorageAdapter for CountingStorageAdapter {
        async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
            let bytes = self.get_bytes(key).await?;
            serde_json::from_slice::<Value>(&bytes).map_err(|e| {
                CoreError::from(StorageError::other(format!("get_json('{key}'): {e}")))
            })
        }

        async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
            let bytes = serde_json::to_vec(value).map_err(|e| {
                CoreError::from(StorageError::other(format!("put_json('{key}'): {e}")))
            })?;
            self.put_bytes(key, &bytes, "application/json").await
        }

        async fn put_json_if_etag_matches(
            &self,
            key: &str,
            value: &Value,
            _expected_etag: Option<&str>,
        ) -> Result<ConditionalWriteStatus, CoreError> {
            if self.force_conflict.load(Ordering::SeqCst) {
                return Ok(ConditionalWriteStatus::Conflict {
                    current_etag: Some("current".to_string()),
                });
            }
            self.put_json(key, value).await?;
            let bytes = serde_json::to_vec(value).map_err(|e| {
                CoreError::from(StorageError::other(format!("put_json('{key}'): {e}")))
            })?;
            Ok(ConditionalWriteStatus::Written {
                etag: Self::etag_for(&bytes),
            })
        }

        async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
            self.get_bytes_calls.fetch_add(1, Ordering::SeqCst);
            self.bytes
                .lock()
                .expect("bytes lock")
                .get(key)
                .cloned()
                .ok_or_else(|| CoreError::from(StorageError::not_found(key)))
        }

        async fn put_bytes(
            &self,
            key: &str,
            bytes: &[u8],
            _content_type: &str,
        ) -> Result<(), CoreError> {
            self.put_bytes_calls.fetch_add(1, Ordering::SeqCst);
            self.seed_bytes(key, bytes);
            Ok(())
        }

        async fn delete_object(&self, key: &str) -> Result<(), CoreError> {
            self.delete_calls.fetch_add(1, Ordering::SeqCst);
            self.bytes.lock().expect("bytes lock").remove(key);
            Ok(())
        }

        async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
            self.head_etag_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .bytes
                .lock()
                .expect("bytes lock")
                .get(key)
                .map(|bytes| Self::etag_for(bytes)))
        }

        async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError> {
            let mut keys: Vec<String> = self
                .bytes
                .lock()
                .expect("bytes lock")
                .keys()
                .filter(|key| key.starts_with(prefix))
                .cloned()
                .collect();
            keys.sort();
            Ok(keys)
        }
    }

    #[test]
    fn storage_retry_summary_truncates_unstructured_errors() {
        let err: CoreError = CoreError::from(StorageError::other("x".repeat(400)));
        let summary = summarize_storage_retry_error(&err);

        assert!(summary.len() <= 243);
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn storage_retry_policy_does_not_retry_not_found() {
        let err: CoreError = CoreError::from(StorageError::not_found("abc"));

        assert!(is_storage_not_found_error(&err));
        assert!(!should_retry_storage_error(&err));
    }

    #[test]
    fn storage_retry_policy_does_not_retry_other() {
        let err: CoreError = CoreError::from(StorageError::other("failed to deserialize JSON"));
        assert!(!should_retry_storage_error(&err));
        assert!(!is_storage_not_found_error(&err));
    }

    #[test]
    fn storage_retry_policy_retries_transient_storage_errors() {
        for err in [
            CoreError::from(StorageError::AuthExpired("ExpiredToken".into())),
            CoreError::from(StorageError::Throttle("SlowDown".into())),
            CoreError::from(StorageError::Timeout("RequestTimeout".into())),
        ] {
            assert!(should_retry_storage_error(&err), "{err}");
        }
    }

    #[tokio::test]
    async fn retry_storage_call_attempts_not_found_once() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result = retry_storage_call("get_bytes", "key", "missing", || {
            let attempts = Arc::clone(&attempts);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(CoreError::from(StorageError::not_found("missing")))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_storage_call_retries_throttle_storage_errors() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result = retry_storage_call("get_bytes", "key", "flaky", || {
            let attempts = Arc::clone(&attempts);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(CoreError::from(StorageError::Throttle("SlowDown".into())))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            super::SAFE_STORAGE_RETRY_DELAYS_MS.len() + 1
        );
    }

    #[tokio::test]
    async fn cache_fetches_bytes_once() {
        let inner = Arc::new(CountingStorageAdapter::default());
        inner.seed_bytes("key", b"value");
        let cache = CachingStorageAdapter::new(inner.clone());

        assert_eq!(cache.get_bytes("key").await.unwrap(), b"value");
        assert_eq!(cache.get_bytes("key").await.unwrap(), b"value");
        assert_eq!(inner.get_bytes_calls(), 1);
    }

    #[tokio::test]
    async fn cache_fetches_json_once() {
        let inner = Arc::new(CountingStorageAdapter::default());
        inner.seed_bytes("key", br#"{"ok":true}"#);
        let cache = CachingStorageAdapter::new(inner.clone());

        assert_eq!(
            cache.get_json("key").await.unwrap(),
            serde_json::json!({"ok": true})
        );
        assert_eq!(
            cache.get_json("key").await.unwrap(),
            serde_json::json!({"ok": true})
        );
        assert_eq!(inner.get_bytes_calls(), 1);
    }

    #[tokio::test]
    async fn cache_fetches_missing_key_once() {
        let inner = Arc::new(CountingStorageAdapter::default());
        let cache = CachingStorageAdapter::new(inner.clone());

        assert!(cache.get_bytes("missing").await.is_err());
        assert!(cache.get_bytes("missing").await.is_err());
        assert_eq!(inner.get_bytes_calls(), 1);
    }

    #[tokio::test]
    async fn cache_updates_after_writes_and_marks_deletes_missing() {
        let inner = Arc::new(CountingStorageAdapter::default());
        let cache = CachingStorageAdapter::new(inner.clone());

        cache
            .put_bytes("key", b"value", "text/plain")
            .await
            .unwrap();
        assert_eq!(cache.get_bytes("key").await.unwrap(), b"value");
        assert_eq!(inner.put_bytes_calls(), 1);
        assert_eq!(inner.get_bytes_calls(), 0);
        assert_eq!(inner.head_etag_calls(), 0);

        cache.delete_object("key").await.unwrap();
        assert!(cache.get_bytes("key").await.is_err());
        assert_eq!(cache.head_etag("key").await.unwrap(), None);
        assert_eq!(inner.delete_calls(), 1);
        assert_eq!(inner.get_bytes_calls(), 0);
        assert_eq!(inner.head_etag_calls(), 0);
    }

    #[tokio::test]
    async fn cache_invalidates_after_conditional_write_conflict() {
        let inner = Arc::new(CountingStorageAdapter::default());
        let cache = CachingStorageAdapter::new(inner.clone());

        cache
            .put_bytes("key", b"cached", "text/plain")
            .await
            .unwrap();
        inner.seed_bytes("key", b"backend");
        inner.force_conflict.store(true, Ordering::SeqCst);

        let status = cache
            .put_json_if_etag_matches("key", &serde_json::json!({"next": true}), Some("stale"))
            .await
            .unwrap();
        assert!(matches!(status, ConditionalWriteStatus::Conflict { .. }));

        inner.force_conflict.store(false, Ordering::SeqCst);
        assert_eq!(cache.get_bytes("key").await.unwrap(), b"backend");
        assert_eq!(inner.get_bytes_calls(), 1);
    }

    #[tokio::test]
    async fn cache_does_not_serve_conditional_put_under_peer_etag() {
        let inner = Arc::new(CountingStorageAdapter::default());
        let cache = CachingStorageAdapter::new(inner.clone());
        let status = cache
            .put_json_if_etag_matches("key", &serde_json::json!({"ours": true}), None)
            .await
            .unwrap();
        assert!(matches!(status, ConditionalWriteStatus::Written { .. }));
        inner.seed_bytes("key", br#"{"peer":true}"#);
        assert_eq!(cache.get_bytes("key").await.unwrap(), br#"{"peer":true}"#);
        assert_eq!(inner.get_bytes_calls(), 1);
    }
}
