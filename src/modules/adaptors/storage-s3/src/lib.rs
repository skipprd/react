use async_trait::async_trait;
use serde_json::Value;

pub use react_core::storage::StorageAdapter;
use react_core::CoreError;

#[derive(Clone)]
pub struct S3StorageAdapter {
    pub bucket: String,
    client: aws_sdk_s3::Client,
}

impl S3StorageAdapter {
    pub async fn from_env(bucket: String) -> Self {
        let aws_cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .load()
            .await;
        let client = aws_sdk_s3::Client::new(&aws_cfg);
        Self { bucket, client }
    }
}

#[async_trait]
impl StorageAdapter for S3StorageAdapter {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        let bytes = self.get_bytes(key).await?;
        serde_json::from_slice::<Value>(&bytes)
            .map_err(|e| CoreError::Storage(e.to_string()))
    }

    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| CoreError::Storage(e.to_string()))?;
        self.put_bytes(key, &bytes, "application/json").await
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| CoreError::Storage(format!("{:?}", e)))?;
        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| CoreError::Storage(format!("{:?}", e)))?
            .into_bytes();
        Ok(bytes.to_vec())
    }

    async fn put_bytes(&self, key: &str, bytes: &[u8], content_type: &str) -> Result<(), CoreError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(aws_sdk_s3::primitives::ByteStream::from(bytes.to_vec()))
            .send()
            .await
            .map_err(|e| CoreError::Storage(format!("{:?}", e)))?;
        Ok(())
    }

    async fn delete_object(&self, key: &str) -> Result<(), CoreError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| CoreError::Storage(format!("{:?}", e)))?;
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
                Err(CoreError::Storage(s))
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
            let resp = req.send().await.map_err(|e| CoreError::Storage(format!("{:?}", e)))?;
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
