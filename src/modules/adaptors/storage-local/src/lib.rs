use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use react_core::error::CoreError;
use react_core::storage::{ConditionalWriteStatus, StorageAdapter};

/// Local filesystem-backed storage adapter.
///
/// Keys are treated as **relative** POSIX-like paths (e.g. `tenant/workspace/project/threads/id.json`)
/// and are stored under `base_dir/<key>`.
#[derive(Clone, Debug)]
pub struct LocalFileStorageAdapter {
    base_dir: Arc<PathBuf>,
}

impl LocalFileStorageAdapter {
    pub fn new(base_dir: impl Into<PathBuf>) -> Result<Self, CoreError> {
        let p: PathBuf = base_dir.into();
        std::fs::create_dir_all(&p).map_err(|e| CoreError::Storage(format!("failed to create base dir: {e}")))?;
        Ok(Self {
            base_dir: Arc::new(p),
        })
    }

    fn resolve_key(&self, key: &str) -> Result<PathBuf, CoreError> {
        let k = key.trim().trim_start_matches('/');
        if k.is_empty() {
            return Err(CoreError::Storage("empty key".to_string()));
        }
        let mut out = (*self.base_dir).clone();
        for seg in k.split('/') {
            if seg.is_empty() {
                return Err(CoreError::Storage("invalid key (empty path segment)".to_string()));
            }
            if seg == "." || seg == ".." || seg.contains("..") {
                return Err(CoreError::Storage("invalid key (path traversal)".to_string()));
            }
            if seg.contains('\\') {
                return Err(CoreError::Storage("invalid key (backslash)".to_string()));
            }
            out.push(seg);
        }
        Ok(out)
    }

    fn rel_key(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(self.base_dir.as_ref()).ok()?;
        let s = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    fn list_files_recursive(&self, dir: &Path, out: &mut Vec<String>) {
        let rd = match std::fs::read_dir(dir) {
            Ok(v) => v,
            Err(_) => return,
        };
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_dir() {
                self.list_files_recursive(&p, out);
            } else if p.is_file() {
                if let Some(k) = self.rel_key(&p) {
                    out.push(k);
                }
            }
        }
    }

    fn etag_for_bytes(bytes: &[u8]) -> String {
        let mut hash: u64 = 1469598103934665603;
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
        format!("fs-etag-{:016x}-{}", hash, bytes.len())
    }
}

#[async_trait]
impl StorageAdapter for LocalFileStorageAdapter {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        let bytes = self.get_bytes(key).await?;
        serde_json::from_slice::<Value>(&bytes)
            .map_err(|e| CoreError::Storage(format!("get_json('{}'): failed to deserialize JSON: {}", key, e)))
    }

    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|e| CoreError::Storage(format!("put_json('{}'): failed to serialize JSON: {}", key, e)))?;
        self.put_bytes(key, &bytes, "application/json").await
    }

    async fn put_json_if_etag_matches(
        &self,
        key: &str,
        value: &Value,
        expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError> {
        let path = self.resolve_key(key)?;
        let bytes = serde_json::to_vec_pretty(value).map_err(|e| {
            CoreError::Storage(format!(
                "put_json_if_etag_matches('{}'): failed to serialize JSON: {}",
                key, e
            ))
        })?;
        let key_owned = key.to_string();
        let expected = expected_etag.map(|s| s.to_string());
        tokio::task::spawn_blocking(move || {
            let current_bytes = match std::fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    return Err(CoreError::Storage(format!(
                        "put_json_if_etag_matches('{}'): read current: {}",
                        key_owned, e
                    )))
                }
            };
            let current_etag = current_bytes
                .as_ref()
                .map(|b| Self::etag_for_bytes(b));
            if current_etag != expected {
                return Ok(ConditionalWriteStatus::Conflict { current_etag });
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    CoreError::Storage(format!(
                        "put_json_if_etag_matches('{}'): mkdir: {}",
                        key_owned, e
                    ))
                })?;
            }
            std::fs::write(&path, &bytes).map_err(|e| {
                CoreError::Storage(format!(
                    "put_json_if_etag_matches('{}'): write: {}",
                    key_owned, e
                ))
            })?;
            Ok(ConditionalWriteStatus::Written)
        })
        .await
        .map_err(|e| {
            CoreError::Storage(format!(
                "put_json_if_etag_matches('{}'): join error: {}",
                key, e
            ))
        })?
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        let path = self.resolve_key(key)?;
        let key_owned = key.to_string();
        tokio::task::spawn_blocking(move || {
            std::fs::read(&path)
                .map_err(|e| CoreError::Storage(format!("get_bytes('{}'): {}", key_owned, e)))
        })
        .await
        .map_err(|e| CoreError::Storage(format!("get_bytes('{}'): join error: {}", key, e)))?
    }

    async fn put_bytes(&self, key: &str, bytes: &[u8], _content_type: &str) -> Result<(), CoreError> {
        let path = self.resolve_key(key)?;
        let b = bytes.to_vec();
        let key_owned = key.to_string();
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| CoreError::Storage(format!("put_bytes('{}'): mkdir: {}", key_owned, e)))?;
            }
            std::fs::write(&path, &b)
                .map_err(|e| CoreError::Storage(format!("put_bytes('{}'): write: {}", key_owned, e)))
        })
        .await
        .map_err(|e| CoreError::Storage(format!("put_bytes('{}'): join error: {}", key, e)))?
    }

    async fn delete_object(&self, key: &str) -> Result<(), CoreError> {
        let path = self.resolve_key(key)?;
        let key_owned = key.to_string();
        tokio::task::spawn_blocking(move || match std::fs::remove_file(&path) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CoreError::Storage(format!("delete_object('{}'): {}", key_owned, e))),
        })
        .await
        .map_err(|e| CoreError::Storage(format!("delete_object('{}'): join error: {}", key, e)))?
    }

    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        let path = self.resolve_key(key)?;
        let key_owned = key.to_string();
        tokio::task::spawn_blocking(move || match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(Self::etag_for_bytes(&bytes))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(CoreError::Storage(format!("head_etag('{}'): {}", key_owned, e))),
        })
        .await
        .map_err(|e| CoreError::Storage(format!("head_etag('{}'): join error: {}", key, e)))?
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError> {
        let pref = prefix.trim().trim_start_matches('/').to_string();
        let adapter = self.clone();
        let prefix_owned = prefix.to_string();
        tokio::task::spawn_blocking(move || {
            let mut keys: Vec<String> = Vec::new();

            let pref_path = PathBuf::from(&pref);
            let start = adapter.base_dir.as_ref().join(&pref_path);
            let (walk_root, filter_prefix) = if start.is_dir() {
                (start, pref.clone())
            } else {
                let parent = pref_path.parent().unwrap_or(Path::new(""));
                (adapter.base_dir.as_ref().join(parent), pref.clone())
            };

            adapter.list_files_recursive(&walk_root, &mut keys);
            keys.retain(|k| k.starts_with(&filter_prefix));
            keys.sort();
            Ok(keys)
        })
        .await
        .map_err(|e| CoreError::Storage(format!("list_prefix('{}'): join error: {}", prefix_owned, e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("react-storage-local-{}", uuid::Uuid::new_v4()));
        p
    }

    #[tokio::test]
    async fn put_get_delete_and_list_prefix() {
        let root = temp_root();
        let st = LocalFileStorageAdapter::new(root.clone()).expect("new");

        st.put_bytes("a/b/c.txt", b"hello", "text/plain")
            .await
            .expect("put");
        st.put_json("a/b/d.json", &serde_json::json!({"x": 1}))
            .await
            .expect("put_json");

        let b = st.get_bytes("a/b/c.txt").await.expect("get");
        assert_eq!(b, b"hello");

        let v = st.get_json("a/b/d.json").await.expect("get_json");
        assert_eq!(v.get("x").and_then(|x| x.as_i64()), Some(1));

        let et = st.head_etag("a/b/c.txt").await.expect("head");
        assert!(et.is_some());

        let keys = st.list_prefix("a/b/").await.expect("list");
        assert_eq!(
            keys,
            vec!["a/b/c.txt".to_string(), "a/b/d.json".to_string()]
        );

        st.delete_object("a/b/c.txt").await.expect("delete");
        assert!(st.head_etag("a/b/c.txt").await.unwrap().is_none());
        assert!(st.get_bytes("a/b/c.txt").await.is_err());

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn rejects_traversal_keys() {
        let root = temp_root();
        let st = LocalFileStorageAdapter::new(root.clone()).expect("new");
        assert!(st.put_bytes("../x", b"nope", "text/plain").await.is_err());
        assert!(st.get_bytes("../x").await.is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn conditional_write_respects_expected_etag() {
        let root = temp_root();
        let st = LocalFileStorageAdapter::new(root.clone()).expect("new");
        let first = serde_json::json!({"v": 1});
        let second = serde_json::json!({"v": 2});

        let created = st
            .put_json_if_etag_matches("a/state.json", &first, None)
            .await
            .expect("create");
        assert_eq!(created, ConditionalWriteStatus::Written);

        let etag = st.head_etag("a/state.json").await.expect("head");
        let conflict = st
            .put_json_if_etag_matches("a/state.json", &second, None)
            .await
            .expect("conflict");
        assert_eq!(conflict, ConditionalWriteStatus::Conflict { current_etag: etag.clone() });

        let updated = st
            .put_json_if_etag_matches("a/state.json", &second, etag.as_deref())
            .await
            .expect("update");
        assert_eq!(updated, ConditionalWriteStatus::Written);
        assert_eq!(st.get_json("a/state.json").await.expect("get"), second);

        let _ = std::fs::remove_dir_all(root);
    }
}
