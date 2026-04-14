use async_trait::async_trait;
use serde_json::Value;

use react_core::storage::ConditionalWriteStatus;
pub use react_core::storage::StorageAdapter;
use react_core::CoreError;

pub struct ObjectMeta {
    pub key: String,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone)]
pub struct S3StorageAdapter {
    pub bucket: String,
    client: aws_sdk_s3::Client,
}

impl S3StorageAdapter {
    fn key_family(path: &str) -> &'static str {
        let normalized = path.trim();
        if normalized.contains("/threads/") {
            return "thread_log";
        }
        if normalized.contains("/state/") {
            if normalized.ends_with("/control.json") {
                return "control_state";
            }
            if normalized.ends_with("/state.json") {
                return "thread_state";
            }
            return "state";
        }
        if normalized.contains("/plans/") {
            if normalized.ends_with("_model.json") {
                return "model_plan";
            }
            if normalized.ends_with("_cleanse.json") {
                return "cleanse_plan";
            }
            return "plan";
        }
        if normalized.contains("/dbt/models/staging/") {
            return "staging_model";
        }
        if normalized.contains("/dbt/models/core/") || normalized.contains("/dbt/models/marts/") {
            return "gold_model";
        }
        if normalized.contains("/dbt/models/") {
            return "dbt_model";
        }
        if normalized.contains("/feedback/") {
            return "feedback";
        }
        if normalized.contains("/logs/") {
            return "run_log";
        }
        if normalized.contains("/dbt/") {
            return "dbt_artifact";
        }
        "artifact"
    }

    fn key_error(&self, op: &str, key: &str, detail: String) -> CoreError {
        CoreError::Storage(format!(
            "s3 {op} failed (bucket='{}', key='{}', key_family='{}'): {detail}",
            self.bucket,
            key,
            Self::key_family(key)
        ))
    }

    fn prefix_error(&self, op: &str, prefix: &str, detail: String) -> CoreError {
        CoreError::Storage(format!(
            "s3 {op} failed (bucket='{}', prefix='{}', key_family='{}'): {detail}",
            self.bucket,
            prefix,
            Self::key_family(prefix)
        ))
    }

    fn conditional_write_error(
        &self,
        op: &str,
        key: &str,
        expected_etag: Option<&str>,
        detail: String,
    ) -> CoreError {
        let condition = if expected_etag.is_some() {
            "if_match"
        } else {
            "if_none_match"
        };
        CoreError::Storage(format!(
            "s3 {op} failed (bucket='{}', key='{}', key_family='{}', condition='{}', expected_etag={:?}): {detail}",
            self.bucket,
            key,
            Self::key_family(key),
            condition,
            expected_etag
        ))
    }

    pub async fn from_env(bucket: String) -> Self {
        let aws_cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .load()
            .await;
        let client = aws_sdk_s3::Client::new(&aws_cfg);
        Self { bucket, client }
    }

    pub async fn list_prefix_meta(&self, prefix: &str) -> Result<Vec<ObjectMeta>, CoreError> {
        let mut token: Option<String> = None;
        let mut out: Vec<ObjectMeta> = Vec::new();
        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix)
                .max_keys(1000);
            if let Some(t) = token.as_ref() {
                req = req.continuation_token(t);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| self.prefix_error("list_objects_v2", prefix, format!("{:?}", e)))?;
            for obj in resp.contents() {
                out.push(ObjectMeta {
                    key: obj.key().unwrap_or_default().to_string(),
                    last_modified: obj.last_modified().and_then(|dt| {
                        chrono::DateTime::from_timestamp(dt.secs(), dt.subsec_nanos())
                    }),
                });
            }
            token = resp.next_continuation_token().map(|s| s.to_string());
            if token.is_none() {
                break;
            }
        }
        Ok(out)
    }

    pub async fn from_credentials(
        bucket: String,
        access_key_id: &str,
        secret_access_key: &str,
        session_token: Option<&str>,
        region: &str,
    ) -> Self {
        let creds = aws_sdk_s3::config::Credentials::new(
            access_key_id,
            secret_access_key,
            session_token.map(|s| s.to_string()),
            None,
            "skippr-auth",
        );
        let s3_config = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .region(aws_sdk_s3::config::Region::new(region.to_string()))
            .credentials_provider(creds)
            .build();
        let client = aws_sdk_s3::Client::from_conf(s3_config);
        Self { bucket, client }
    }
}

#[async_trait]
impl StorageAdapter for S3StorageAdapter {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        let bytes = self.get_bytes(key).await?;
        serde_json::from_slice::<Value>(&bytes).map_err(|e| CoreError::Storage(e.to_string()))
    }

    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec(value).map_err(|e| CoreError::Storage(e.to_string()))?;
        self.put_bytes(key, &bytes, "application/json").await
    }

    async fn put_json_if_etag_matches(
        &self,
        key: &str,
        value: &Value,
        expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError> {
        let bytes = serde_json::to_vec(value).map_err(|e| CoreError::Storage(e.to_string()))?;
        let mut req = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type("application/json")
            .body(aws_sdk_s3::primitives::ByteStream::from(bytes));
        req = match expected_etag {
            Some(etag) => req.if_match(etag),
            None => req.if_none_match("*"),
        };
        match req.send().await {
            Ok(_) => Ok(ConditionalWriteStatus::Written),
            Err(e) => {
                let s = format!("{:?}", e);
                if s.contains("PreconditionFailed") || s.contains("ConditionalRequestConflict") {
                    let current_etag = self.head_etag(key).await?;
                    return Ok(ConditionalWriteStatus::Conflict { current_etag });
                }
                Err(self.conditional_write_error("put_object", key, expected_etag, s))
            }
        }
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| self.key_error("get_object", key, format!("{:?}", e)))?;
        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| self.key_error("get_object_body", key, format!("{:?}", e)))?
            .into_bytes();
        Ok(bytes.to_vec())
    }

    async fn put_bytes(
        &self,
        key: &str,
        bytes: &[u8],
        content_type: &str,
    ) -> Result<(), CoreError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(aws_sdk_s3::primitives::ByteStream::from(bytes.to_vec()))
            .send()
            .await
            .map_err(|e| self.key_error("put_object", key, format!("{:?}", e)))?;
        Ok(())
    }

    async fn delete_object(&self, key: &str) -> Result<(), CoreError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| self.key_error("delete_object", key, format!("{:?}", e)))?;
        Ok(())
    }

    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        let resp = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await;
        match resp {
            Ok(r) => Ok(r.e_tag().map(|s| s.to_string())),
            Err(e) => {
                let s = format!("{:?}", e);
                if s.contains("NoSuchKey") || s.contains("NotFound") {
                    return Ok(None);
                }
                Err(self.key_error("head_object", key, s))
            }
        }
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError> {
        let mut token: Option<String> = None;
        let mut out: Vec<String> = Vec::new();
        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix)
                .max_keys(1000);
            if let Some(t) = token.as_ref() {
                req = req.continuation_token(t);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| self.prefix_error("list_objects_v2", prefix, format!("{:?}", e)))?;
            for obj in resp.contents() {
                if let Some(k) = obj.key() {
                    out.push(k.to_string());
                }
            }
            token = resp.next_continuation_token().map(|s| s.to_string());
            if token.is_none() {
                break;
            }
        }
        Ok(out)
    }
}
