use async_trait::async_trait;
use serde_json::Value;

use crate::error::CoreError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionalWriteStatus {
    Written,
    Conflict { current_etag: Option<String> },
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
    async fn put_bytes(&self, key: &str, bytes: &[u8], content_type: &str) -> Result<(), CoreError>;

    async fn delete_object(&self, key: &str) -> Result<(), CoreError>;
    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError>;

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError>;
}
