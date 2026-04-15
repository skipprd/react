use async_trait::async_trait;
use serde_json::Value;
use std::future::Future;
use std::time::Duration;

use crate::error::CoreError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionalWriteStatus {
    Written,
    Conflict { current_etag: Option<String> },
}

pub const SAFE_STORAGE_RETRY_DELAYS_MS: [u64; 3] = [25, 75, 150];

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

pub fn should_retry_storage_error(err: &CoreError) -> bool {
    matches!(err, CoreError::Storage(_))
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
                    error = %err,
                    "storage operation failed; retrying"
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
