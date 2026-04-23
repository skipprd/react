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
    let response_message = extract_quoted_detail_field(detail, &["message: Some(\"", "message: \""]);
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
            return decorate_storage_retry_code(code, response_message.as_deref(), response_key.as_deref());
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
    use super::{summarize_storage_retry_error, truncate_for_log};
    use crate::error::CoreError;

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
}
