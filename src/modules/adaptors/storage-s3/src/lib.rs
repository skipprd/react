use async_trait::async_trait;
use serde_json::Value;
use std::future::Future;
use std::sync::{Arc, RwLock};

use react_core::resolved_config::S3Credentials;
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
    client: Arc<RwLock<aws_sdk_s3::Client>>,
    credentials_provider: Option<Arc<dyn react_core::resolved_config::S3CredentialsProvider>>,
}

impl S3StorageAdapter {
    fn wrap_client(client: aws_sdk_s3::Client) -> Arc<RwLock<aws_sdk_s3::Client>> {
        Arc::new(RwLock::new(client))
    }

    fn current_client(&self) -> aws_sdk_s3::Client {
        self.client.read().expect("s3 client lock poisoned").clone()
    }

    fn client_from_credentials(credentials: &S3Credentials) -> aws_sdk_s3::Client {
        let creds = aws_sdk_s3::config::Credentials::new(
            &credentials.access_key_id,
            &credentials.secret_access_key,
            credentials.session_token.clone(),
            None,
            "skippr-auth",
        );
        let s3_config = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .region(aws_sdk_s3::config::Region::new(credentials.region.clone()))
            .credentials_provider(creds)
            .build();
        aws_sdk_s3::Client::from_conf(s3_config)
    }

    async fn refresh_client(&self) -> Result<(), CoreError> {
        let Some(provider) = self.credentials_provider.as_ref() else {
            return Ok(());
        };
        let credentials = provider.s3_credentials().await.map_err(|e| {
            CoreError::Storage(format!(
                "s3 credential refresh failed (bucket='{}'): {e}",
                self.bucket
            ))
        })?;
        let client = Self::client_from_credentials(&credentials);
        *self.client.write().expect("s3 client lock poisoned") = client;
        Ok(())
    }

    fn is_expired_token_error(err: &CoreError) -> bool {
        matches!(err, CoreError::Storage(message) if message.contains("ExpiredToken"))
    }

    async fn request_with_refresh<T, F, Fut>(&self, request: F) -> Result<T, CoreError>
    where
        F: Fn(aws_sdk_s3::Client) -> Fut,
        Fut: Future<Output = Result<T, CoreError>>,
    {
        let first = request(self.current_client()).await;
        if !matches!(first, Err(ref err) if Self::is_expired_token_error(err))
            || self.credentials_provider.is_none()
        {
            return first;
        }

        self.refresh_client().await?;
        request(self.current_client()).await
    }

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
        Self {
            bucket,
            client: Self::wrap_client(client),
            credentials_provider: None,
        }
    }

    pub async fn list_prefix_meta(&self, prefix: &str) -> Result<Vec<ObjectMeta>, CoreError> {
        let mut token: Option<String> = None;
        let mut out: Vec<ObjectMeta> = Vec::new();
        loop {
            let page_token = token.clone();
            let bucket = self.bucket.clone();
            let resp = self
                .request_with_refresh(|client| {
                    let bucket = bucket.clone();
                    let page_token = page_token.clone();
                    async move {
                        let mut req = client
                            .list_objects_v2()
                            .bucket(bucket)
                            .prefix(prefix)
                            .max_keys(1000);
                        if let Some(t) = page_token.as_ref() {
                            req = req.continuation_token(t);
                        }
                        req.send().await.map_err(|e| {
                            self.prefix_error("list_objects_v2", prefix, format!("{:?}", e))
                        })
                    }
                })
                .await?;
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
        let credentials = S3Credentials {
            access_key_id: access_key_id.to_string(),
            secret_access_key: secret_access_key.to_string(),
            session_token: session_token.map(|s| s.to_string()),
            region: region.to_string(),
            expires_at: None,
            provider: None,
        };
        Self::from_resolved_credentials(bucket, &credentials).await
    }

    pub async fn from_resolved_credentials(bucket: String, credentials: &S3Credentials) -> Self {
        let client = Self::client_from_credentials(credentials);
        Self {
            bucket,
            client: Self::wrap_client(client),
            credentials_provider: credentials.provider.clone(),
        }
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
        let bucket = self.bucket.clone();
        let expected = expected_etag.map(str::to_string);
        match self
            .request_with_refresh(|client| {
                let bucket = bucket.clone();
                let bytes = bytes.clone();
                let expected = expected.clone();
                async move {
                    let mut req = client
                        .put_object()
                        .bucket(bucket)
                        .key(key)
                        .content_type("application/json")
                        .body(aws_sdk_s3::primitives::ByteStream::from(bytes));
                    req = match expected.as_deref() {
                        Some(etag) => req.if_match(etag),
                        None => req.if_none_match("*"),
                    };
                    req.send().await.map(|_| ()).map_err(|e| {
                        self.conditional_write_error(
                            "put_object",
                            key,
                            expected_etag,
                            format!("{:?}", e),
                        )
                    })
                }
            })
            .await
        {
            Ok(_) => Ok(ConditionalWriteStatus::Written),
            Err(e) => {
                let s = e.to_string();
                if s.contains("PreconditionFailed") || s.contains("ConditionalRequestConflict") {
                    let current_etag = self.head_etag(key).await?;
                    return Ok(ConditionalWriteStatus::Conflict { current_etag });
                }
                Err(e)
            }
        }
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        let bucket = self.bucket.clone();
        let resp = self
            .request_with_refresh(|client| {
                let bucket = bucket.clone();
                async move {
                    client
                        .get_object()
                        .bucket(bucket)
                        .key(key)
                        .send()
                        .await
                        .map_err(|e| self.key_error("get_object", key, format!("{:?}", e)))
                }
            })
            .await?;
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
        let bucket = self.bucket.clone();
        let body = bytes.to_vec();
        self.request_with_refresh(|client| {
            let bucket = bucket.clone();
            let body = body.clone();
            async move {
                client
                    .put_object()
                    .bucket(bucket)
                    .key(key)
                    .content_type(content_type)
                    .body(aws_sdk_s3::primitives::ByteStream::from(body))
                    .send()
                    .await
                    .map(|_| ())
                    .map_err(|e| self.key_error("put_object", key, format!("{:?}", e)))
            }
        })
        .await
    }

    async fn delete_object(&self, key: &str) -> Result<(), CoreError> {
        let bucket = self.bucket.clone();
        self.request_with_refresh(|client| {
            let bucket = bucket.clone();
            async move {
                client
                    .delete_object()
                    .bucket(bucket)
                    .key(key)
                    .send()
                    .await
                    .map(|_| ())
                    .map_err(|e| self.key_error("delete_object", key, format!("{:?}", e)))
            }
        })
        .await
    }

    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        let bucket = self.bucket.clone();
        let resp = self
            .request_with_refresh(|client| {
                let bucket = bucket.clone();
                async move {
                    client
                        .head_object()
                        .bucket(bucket)
                        .key(key)
                        .send()
                        .await
                        .map_err(|e| self.key_error("head_object", key, format!("{:?}", e)))
                }
            })
            .await;
        match resp {
            Ok(r) => Ok(r.e_tag().map(|s| s.to_string())),
            Err(e) => {
                let s = e.to_string();
                if s.contains("NoSuchKey") || s.contains("NotFound") {
                    return Ok(None);
                }
                Err(e)
            }
        }
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError> {
        let mut token: Option<String> = None;
        let mut out: Vec<String> = Vec::new();
        loop {
            let page_token = token.clone();
            let bucket = self.bucket.clone();
            let resp = self
                .request_with_refresh(|client| {
                    let bucket = bucket.clone();
                    let page_token = page_token.clone();
                    async move {
                        let mut req = client
                            .list_objects_v2()
                            .bucket(bucket)
                            .prefix(prefix)
                            .max_keys(1000);
                        if let Some(t) = page_token.as_ref() {
                            req = req.continuation_token(t);
                        }
                        req.send().await.map_err(|e| {
                            self.prefix_error("list_objects_v2", prefix, format!("{:?}", e))
                        })
                    }
                })
                .await?;
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
