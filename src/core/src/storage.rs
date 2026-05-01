use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::error::CoreError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionalWriteStatus {
    Written,
    Conflict { current_etag: Option<String> },
}

pub const SAFE_STORAGE_RETRY_DELAYS_MS: [u64; 3] = [25, 75, 150];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageErrorKind {
    NotFound,
    AuthExpired,
    Throttle,
    Timeout,
    Unknown,
}

#[derive(Clone, Debug)]
enum CacheEntry {
    Bytes(Vec<u8>),
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
            CoreError::Storage(format!("storage cache read lock poisoned for '{key}'"))
        })?;
        Ok(entries.get(key).cloned())
    }

    fn write_entry(&self, key: &str, entry: CacheEntry) -> Result<(), CoreError> {
        let mut entries = self.entries.write().map_err(|_| {
            CoreError::Storage(format!("storage cache write lock poisoned for '{key}'"))
        })?;
        entries.insert(key.to_string(), entry);
        Ok(())
    }

    fn invalidate_entry(&self, key: &str) -> Result<(), CoreError> {
        let mut entries = self.entries.write().map_err(|_| {
            CoreError::Storage(format!("storage cache write lock poisoned for '{key}'"))
        })?;
        entries.remove(key);
        Ok(())
    }

    fn cached_missing_error(key: &str) -> CoreError {
        CoreError::Storage(format!("cached storage miss for key '{key}': not found"))
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
            CoreError::Storage(format!(
                "get_json('{key}'): failed to deserialize JSON: {e}"
            ))
        })
    }

    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec(value).map_err(|e| {
            CoreError::Storage(format!("put_json('{key}'): failed to serialize JSON: {e}"))
        })?;
        self.inner
            .put_bytes(key, &bytes, "application/json")
            .await?;
        self.write_entry(key, CacheEntry::Bytes(bytes))
    }

    async fn put_json_if_etag_matches(
        &self,
        key: &str,
        value: &Value,
        expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError> {
        let bytes = serde_json::to_vec(value).map_err(|e| {
            CoreError::Storage(format!(
                "put_json_if_etag_matches('{key}'): failed to serialize JSON: {e}"
            ))
        })?;
        let status = self
            .inner
            .put_json_if_etag_matches(key, value, expected_etag)
            .await?;
        match status {
            ConditionalWriteStatus::Written => {
                self.write_entry(key, CacheEntry::Bytes(bytes))?;
                Ok(ConditionalWriteStatus::Written)
            }
            ConditionalWriteStatus::Conflict { current_etag } => {
                self.invalidate_entry(key)?;
                Ok(ConditionalWriteStatus::Conflict { current_etag })
            }
        }
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        match self.read_entry(key)? {
            Some(CacheEntry::Bytes(bytes)) => return Ok(bytes),
            Some(CacheEntry::Missing) => return Err(Self::cached_missing_error(key)),
            None => {}
        }

        match self.inner.get_bytes(key).await {
            Ok(bytes) => {
                self.write_entry(key, CacheEntry::Bytes(bytes.clone()))?;
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
        self.write_entry(key, CacheEntry::Bytes(bytes.to_vec()))
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

pub fn classify_storage_error(err: &CoreError) -> Option<StorageErrorKind> {
    match err {
        CoreError::Storage(message) => Some(classify_storage_error_message(message)),
        _ => None,
    }
}

pub fn is_storage_not_found_error(err: &CoreError) -> bool {
    matches!(
        classify_storage_error(err),
        Some(StorageErrorKind::NotFound)
    )
}

fn classify_storage_error_message(message: &str) -> StorageErrorKind {
    let lower = message.to_ascii_lowercase();

    if lower.contains("nosuchkey")
        || lower.contains("no such key")
        || lower.contains("notfound")
        || lower.contains("not found")
        || lower.contains("no such file or directory")
        || lower.contains("os error 2")
        || lower.contains("status code: 404")
        || lower.contains("404 not found")
    {
        return StorageErrorKind::NotFound;
    }
    if lower.contains("expiredtoken") || lower.contains("token has expired") {
        return StorageErrorKind::AuthExpired;
    }
    if lower.contains("slowdown")
        || lower.contains("throttl")
        || lower.contains("too many requests")
        || lower.contains("rate exceeded")
        || lower.contains("requestlimitexceeded")
    {
        return StorageErrorKind::Throttle;
    }
    if lower.contains("timeout") || lower.contains("timed out") || lower.contains("requesttimeout")
    {
        return StorageErrorKind::Timeout;
    }

    StorageErrorKind::Unknown
}

pub fn should_retry_storage_error(err: &CoreError) -> bool {
    matches!(
        classify_storage_error(err),
        Some(
            StorageErrorKind::AuthExpired
                | StorageErrorKind::Throttle
                | StorageErrorKind::Timeout
                | StorageErrorKind::Unknown
        )
    )
}

fn truncate_for_log(value: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(value.len().min(max_chars) + 3);
    for (idx, ch) in value.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

fn summarize_storage_retry_error(err: &CoreError) -> String {
    match err {
        CoreError::Storage(message) => summarize_storage_retry_message(message),
        _ => truncate_for_log(&err.to_string(), 240),
    }
}

fn summarize_storage_retry_message(message: &str) -> String {
    let trimmed = message.trim();
    let raw_detail_markers = [
        ": ServiceError(",
        ": DispatchFailure(",
        ": ResponseError(",
        ": TimeoutError(",
        ": ConstructionFailure(",
    ];

    for marker in raw_detail_markers {
        if let Some((context, detail)) = trimmed.split_once(marker) {
            return format!(
                "{}: {}",
                context.trim_end(),
                summarize_storage_retry_detail(marker.trim_start_matches(": "), detail)
            );
        }
    }

    truncate_for_log(trimmed, 240)
}

fn summarize_storage_retry_detail(kind: &str, detail: &str) -> String {
    let compact_kind = kind.trim_end_matches('(');
    let detail = detail.trim();
    let response_message =
        extract_quoted_detail_field(detail, &["message: Some(\"", "message: \""]);
    let response_key = extract_quoted_detail_field(detail, &["key: Some(\"", "key: \""]);

    let known_codes = [
        "NoSuchKey",
        "NotFound",
        "AccessDenied",
        "ExpiredToken",
        "InvalidAccessKeyId",
        "SlowDown",
        "Throttling",
        "RequestTimeout",
    ];
    for code in known_codes {
        if detail.contains(code) {
            return decorate_storage_retry_code(
                code,
                response_message.as_deref(),
                response_key.as_deref(),
            );
        }
    }

    if detail.contains("ConnectorError") {
        return format!("{compact_kind}: connector error");
    }
    if detail.contains("Timeout") {
        return format!("{compact_kind}: timeout");
    }
    if detail.contains("Credentials") || detail.contains("credential") {
        return format!("{compact_kind}: credentials error");
    }

    format!("{compact_kind}: {}", truncate_for_log(detail, 120))
}

fn extract_quoted_detail_field(detail: &str, patterns: &[&str]) -> Option<String> {
    for pattern in patterns {
        if let Some((_, rest)) = detail.split_once(pattern) {
            if let Some((value, _)) = rest.split_once('"') {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn decorate_storage_retry_code(
    code: &str,
    response_message: Option<&str>,
    response_key: Option<&str>,
) -> String {
    let mut extras = Vec::new();
    if let Some(message) = response_message {
        extras.push(format!("message: {}", truncate_for_log(message, 120)));
    }
    if let Some(key) = response_key {
        extras.push(format!("response_key: {}", truncate_for_log(key, 120)));
    }
    if extras.is_empty() {
        code.to_string()
    } else {
        format!("{code} ({})", extras.join(", "))
    }
}

pub async fn retry_storage_call<T, Fut, F>(
    operation: &'static str,
    target_kind: &'static str,
    target: &str,
    mut call: F,
) -> Result<T, CoreError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, CoreError>>,
{
    let mut retry_count = 0usize;
    loop {
        match call().await {
            Ok(value) => {
                if retry_count > 0 {
                    tracing::info!(
                        operation,
                        target_kind,
                        target,
                        retry_count,
                        "storage retry succeeded"
                    );
                }
                return Ok(value);
            }
            Err(err) => {
                if !should_retry_storage_error(&err)
                    || retry_count == SAFE_STORAGE_RETRY_DELAYS_MS.len()
                {
                    return Err(err);
                }
                let backoff_ms = SAFE_STORAGE_RETRY_DELAYS_MS[retry_count];
                retry_count += 1;
                tracing::warn!(
                    operation,
                    target_kind,
                    target,
                    retry_count,
                    max_retries = SAFE_STORAGE_RETRY_DELAYS_MS.len(),
                    backoff_ms,
                    error = %summarize_storage_retry_error(&err),
                    "storage operation failed; retrying"
                );
                tracing::debug!(
                    operation,
                    target_kind,
                    target,
                    retry_count,
                    backoff_ms,
                    full_error = %err,
                    "storage retry raw error detail"
                );
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
        }
    }
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
        summarize_storage_retry_error, truncate_for_log, CachingStorageAdapter,
        ConditionalWriteStatus, StorageAdapter,
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
            serde_json::from_slice::<Value>(&bytes)
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
            _expected_etag: Option<&str>,
        ) -> Result<ConditionalWriteStatus, CoreError> {
            if self.force_conflict.load(Ordering::SeqCst) {
                return Ok(ConditionalWriteStatus::Conflict {
                    current_etag: Some("current".to_string()),
                });
            }
            self.put_json(key, value).await?;
            Ok(ConditionalWriteStatus::Written)
        }

        async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
            self.get_bytes_calls.fetch_add(1, Ordering::SeqCst);
            self.bytes
                .lock()
                .expect("bytes lock")
                .get(key)
                .cloned()
                .ok_or_else(|| CoreError::Storage(format!("get_bytes('{key}'): not found")))
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
                .contains_key(key)
                .then(|| "etag".to_string()))
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
    fn storage_retry_summary_extracts_s3_error_code() {
        let err = CoreError::Storage("s3 get_object failed (bucket='skippr-prod', key='abc', key_family='thread_log'): ServiceError(ServiceError { source: NoSuchKey(NoSuchKey { message: Some(\"The specified key does not exist.\"), meta: ErrorMetadata { code: Some(\"NoSuchKey\") } }) })".to_string());

        assert_eq!(
            summarize_storage_retry_error(&err),
            "s3 get_object failed (bucket='skippr-prod', key='abc', key_family='thread_log'): NoSuchKey (message: The specified key does not exist.)"
        );
    }

    #[test]
    fn storage_retry_summary_includes_response_key_when_present() {
        let err = CoreError::Storage("s3 get_object failed (bucket='skippr-prod', key='abc', key_family='thread_log'): ServiceError(ServiceError { source: NoSuchKey(NoSuchKey { message: Some(\"The specified key does not exist.\"), key: Some(\"missing/thread.json\"), meta: ErrorMetadata { code: Some(\"NoSuchKey\") } }) })".to_string());

        assert_eq!(
            summarize_storage_retry_error(&err),
            "s3 get_object failed (bucket='skippr-prod', key='abc', key_family='thread_log'): NoSuchKey (message: The specified key does not exist., response_key: missing/thread.json)"
        );
    }

    #[test]
    fn storage_retry_summary_truncates_unstructured_errors() {
        let err = CoreError::Storage("x".repeat(400));
        let summary = summarize_storage_retry_error(&err);

        assert!(summary.len() <= 243);
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn truncate_for_log_keeps_short_strings_unchanged() {
        assert_eq!(truncate_for_log("short", 10), "short");
    }

    #[test]
    fn storage_retry_policy_does_not_retry_not_found() {
        let err = CoreError::Storage("ServiceError: NoSuchKey".to_string());

        assert!(is_storage_not_found_error(&err));
        assert!(!should_retry_storage_error(&err));
    }

    #[test]
    fn storage_retry_policy_retries_transient_storage_errors() {
        for message in [
            "ServiceError: ExpiredToken",
            "ServiceError: SlowDown",
            "DispatchFailure: connector error",
            "ServiceError: service unavailable",
        ] {
            assert!(
                should_retry_storage_error(&CoreError::Storage(message.to_string())),
                "{message}"
            );
        }
    }

    #[tokio::test]
    async fn retry_storage_call_attempts_not_found_once() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result = retry_storage_call("get_bytes", "key", "missing", || {
            let attempts = Arc::clone(&attempts);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(CoreError::Storage("NoSuchKey".to_string()))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_storage_call_retries_unknown_storage_errors() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result = retry_storage_call("get_bytes", "key", "flaky", || {
            let attempts = Arc::clone(&attempts);
            async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(CoreError::Storage("service unavailable".to_string()))
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
}
