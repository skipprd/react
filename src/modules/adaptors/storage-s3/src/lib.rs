use async_trait::async_trait;
use serde_json::Value;
use std::future::Future;
use std::sync::{Arc, RwLock};

use react_core::resolved_config::S3Credentials;
pub use react_core::storage::StorageAdapter;
use react_core::storage::{is_storage_not_found_error, ConditionalWriteStatus, StorageError};
use react_core::CoreError;

pub const DEFAULT_RUNTIME_BUCKET: &str = "skippr-react-runtime";
pub const DEFAULT_VECTORS_BUCKET: &str = "skippr-react-vectors";

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|v| {
        let t = v.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

/// Cloudflare R2 credentials from Skippr Cloud `OBJECTS_*` env (R7).
/// Does **not** read the AWS default credential chain.
pub fn objects_credentials_from_env() -> Result<S3Credentials, String> {
    let access_key_id = env_nonempty("OBJECTS_ACCESS_KEY_ID")
        .ok_or_else(|| "missing OBJECTS_ACCESS_KEY_ID (R2 S3 API key)".to_string())?;
    let secret_access_key = env_nonempty("OBJECTS_SECRET_ACCESS_KEY")
        .ok_or_else(|| "missing OBJECTS_SECRET_ACCESS_KEY (R2 S3 API secret)".to_string())?;
    let endpoint = env_nonempty("OBJECTS_S3_ENDPOINT")
        .ok_or_else(|| "missing OBJECTS_S3_ENDPOINT (R2 S3 API endpoint)".to_string())?;
    let region = env_nonempty("OBJECTS_S3_REGION").unwrap_or_else(|| "auto".to_string());
    Ok(S3Credentials {
        access_key_id,
        secret_access_key,
        session_token: None,
        region,
        endpoint: Some(endpoint),
        force_path_style: true,
        expires_at: None,
        provider: None,
    })
}

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
            "skippr-objects",
        );
        let mut builder = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .region(aws_sdk_s3::config::Region::new(credentials.region.clone()))
            .credentials_provider(creds)
            .force_path_style(credentials.force_path_style);
        if let Some(endpoint) = credentials.endpoint.as_deref() {
            builder = builder.endpoint_url(endpoint);
        }
        aws_sdk_s3::Client::from_conf(builder.build())
    }

    async fn refresh_client(&self) -> Result<(), CoreError> {
        let Some(provider) = self.credentials_provider.as_ref() else {
            return Ok(());
        };
        let credentials = provider.s3_credentials().await.map_err(|e| {
            CoreError::from(StorageError::other(format!(
                "s3 credential refresh failed (bucket='{}'): {e}",
                self.bucket
            )))
        })?;
        let client = Self::client_from_credentials(&credentials);
        *self.client.write().expect("s3 client lock poisoned") = client;
        Ok(())
    }

    fn is_expired_token_error(err: &CoreError) -> bool {
        matches!(err, CoreError::Storage(StorageError::AuthExpired(_)))
    }

    fn map_sdk_error<E, R>(err: aws_sdk_s3::error::SdkError<E, R>) -> StorageError
    where
        E: std::fmt::Debug + aws_sdk_s3::error::ProvideErrorMetadata,
        R: std::fmt::Debug,
    {
        match &err {
            aws_sdk_s3::error::SdkError::TimeoutError(_) => {
                StorageError::Timeout(format!("{err:?}"))
            }
            aws_sdk_s3::error::SdkError::DispatchFailure(_) => {
                StorageError::Throttle(format!("{err:?}"))
            }
            aws_sdk_s3::error::SdkError::ServiceError(ctx) => {
                match ctx.err().code().unwrap_or("") {
                    "NoSuchKey" | "NotFound" | "NoSuchBucket" => {
                        StorageError::NotFound(ctx.err().code().unwrap_or("not found").to_string())
                    }
                    "ExpiredToken" | "ExpiredTokenException" | "InvalidToken" => {
                        StorageError::AuthExpired(ctx.err().code().unwrap_or("").to_string())
                    }
                    "SlowDown"
                    | "Throttling"
                    | "ThrottlingException"
                    | "RequestLimitExceeded"
                    | "TooManyRequestsException" => {
                        StorageError::Throttle(ctx.err().code().unwrap_or("").to_string())
                    }
                    "RequestTimeout" => {
                        StorageError::Timeout(ctx.err().code().unwrap_or("").to_string())
                    }
                    "PreconditionFailed" | "ConditionalRequestConflict" => {
                        StorageError::Conflict(ctx.err().code().unwrap_or("").to_string())
                    }
                    _ => StorageError::Other(format!("{err:?}")),
                }
            }
            _ => StorageError::Other(format!("{err:?}")),
        }
    }

    fn key_error<E, R>(
        &self,
        _op: &str,
        key: &str,
        err: aws_sdk_s3::error::SdkError<E, R>,
    ) -> CoreError
    where
        E: std::fmt::Debug + aws_sdk_s3::error::ProvideErrorMetadata,
        R: std::fmt::Debug,
    {
        match Self::map_sdk_error(err) {
            StorageError::NotFound(_) => CoreError::from(StorageError::NotFound(key.to_string())),
            other => CoreError::from(other),
        }
    }

    fn prefix_error<E, R>(
        &self,
        _op: &str,
        prefix: &str,
        err: aws_sdk_s3::error::SdkError<E, R>,
    ) -> CoreError
    where
        E: std::fmt::Debug + aws_sdk_s3::error::ProvideErrorMetadata,
        R: std::fmt::Debug,
    {
        match Self::map_sdk_error(err) {
            StorageError::NotFound(_) => {
                CoreError::from(StorageError::NotFound(prefix.to_string()))
            }
            other => CoreError::from(other),
        }
    }

    fn conditional_write_error<E, R>(
        &self,
        _op: &str,
        key: &str,
        _expected_etag: Option<&str>,
        err: aws_sdk_s3::error::SdkError<E, R>,
    ) -> CoreError
    where
        E: std::fmt::Debug + aws_sdk_s3::error::ProvideErrorMetadata,
        R: std::fmt::Debug,
    {
        match Self::map_sdk_error(err) {
            StorageError::NotFound(_) => CoreError::from(StorageError::NotFound(key.to_string())),
            other => CoreError::from(other),
        }
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

    pub async fn from_env(bucket: String) -> Result<Self, String> {
        let credentials = objects_credentials_from_env()?;
        Ok(Self::from_resolved_credentials(bucket, &credentials).await)
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
                        req.send()
                            .await
                            .map_err(|e| self.prefix_error("list_objects_v2", prefix, e))
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
            endpoint: None,
            force_path_style: false,
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
        serde_json::from_slice::<Value>(&bytes)
            .map_err(|e| CoreError::from(StorageError::other(e.to_string())))
    }

    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| CoreError::from(StorageError::other(e.to_string())))?;
        self.put_bytes(key, &bytes, "application/json").await
    }

    async fn put_json_if_etag_matches(
        &self,
        key: &str,
        value: &Value,
        expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError> {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| CoreError::from(StorageError::other(e.to_string())))?;
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
                    let out = req.send().await.map_err(|e| {
                        self.conditional_write_error("put_object", key, expected_etag, e)
                    })?;
                    out.e_tag().map(|s| s.to_string()).ok_or_else(|| {
                        CoreError::from(StorageError::other(format!(
                            "put_object('{key}'): response missing etag"
                        )))
                    })
                }
            })
            .await
        {
            Ok(etag) => Ok(ConditionalWriteStatus::Written { etag }),
            Err(e) => {
                if matches!(e, CoreError::Storage(StorageError::Conflict(_))) {
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
                        .map_err(|e| self.key_error("get_object", key, e))
                }
            })
            .await?;
        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| {
                CoreError::from(StorageError::other(format!(
                    "s3 get_object_body failed (key='{key}'): {e:?}"
                )))
            })?
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
                    .map_err(|e| self.key_error("put_object", key, e))
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
                    .map_err(|e| self.key_error("delete_object", key, e))
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
                        .map_err(|e| self.key_error("head_object", key, e))
                }
            })
            .await;
        match resp {
            Ok(r) => Ok(r.e_tag().map(|s| s.to_string())),
            Err(e) if is_storage_not_found_error(&e) => Ok(None),
            Err(e) => Err(e),
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
                        req.send()
                            .await
                            .map_err(|e| self.prefix_error("list_objects_v2", prefix, e))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_credentials_require_r2_env() {
        let saved = [
            "OBJECTS_ACCESS_KEY_ID",
            "OBJECTS_SECRET_ACCESS_KEY",
            "OBJECTS_S3_ENDPOINT",
            "OBJECTS_S3_REGION",
        ]
        .map(|k| (k, std::env::var(k).ok()));
        for (k, _) in &saved {
            std::env::remove_var(k);
        }
        let err = objects_credentials_from_env().expect_err("missing env must fail");
        assert!(err.contains("OBJECTS_ACCESS_KEY_ID"), "{err}");

        std::env::set_var("OBJECTS_ACCESS_KEY_ID", "ak");
        std::env::set_var("OBJECTS_SECRET_ACCESS_KEY", "sk");
        std::env::set_var(
            "OBJECTS_S3_ENDPOINT",
            "https://example.r2.cloudflarestorage.com",
        );
        std::env::set_var("OBJECTS_S3_REGION", "auto");
        let creds = objects_credentials_from_env().expect("OBJECTS_* present");
        assert_eq!(creds.access_key_id, "ak");
        assert_eq!(creds.region, "auto");
        assert_eq!(
            creds.endpoint.as_deref(),
            Some("https://example.r2.cloudflarestorage.com")
        );
        assert!(creds.force_path_style);
        for (k, v) in saved {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}
