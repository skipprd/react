use crate::error::CoreError;
use crate::scope::{ensure_safe_scope_segment, RequestScope};

fn ks_err(e: CoreError) -> CoreError {
    CoreError::Keyspace(e.to_string())
}

/// Percent-encode non-alphanumeric characters (except `-`, `_`, `.`) so that
/// arbitrary identifiers (e.g. `entity.group.item`) can be used as path segments.
pub fn encode_key_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        let c = *b as char;
        let safe = c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.';
        if safe {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub trait Keyspace: Send + Sync {
    /// Build a scoped storage key from path segments.
    fn scoped_key(&self, scope: &RequestScope, segments: &[&str]) -> String;
    /// Build a scoped prefix (directory-like, trailing slash) from path segments.
    fn scoped_prefix(&self, scope: &RequestScope, segments: &[&str]) -> String;

    fn threads_prefix(&self, scope: &RequestScope) -> String {
        self.scoped_prefix(scope, &["threads"])
    }
    fn thread_key(&self, scope: &RequestScope, thread_id: &str) -> Result<String, CoreError> {
        ensure_safe_scope_segment("thread_id", thread_id).map_err(ks_err)?;
        Ok(self.scoped_key(scope, &["threads", &format!("{thread_id}.json")]))
    }
    fn thread_state_key(&self, scope: &RequestScope, thread_id: &str) -> Result<String, CoreError> {
        ensure_safe_scope_segment("thread_id", thread_id).map_err(ks_err)?;
        Ok(self.scoped_key(scope, &["state", thread_id, "state.json"]))
    }
    fn control_state_key(
        &self,
        scope: &RequestScope,
        thread_id: &str,
    ) -> Result<String, CoreError> {
        ensure_safe_scope_segment("thread_id", thread_id).map_err(ks_err)?;
        Ok(self.scoped_key(scope, &["state", thread_id, "control.json"]))
    }
    fn thread_artifact_key(
        &self,
        scope: &RequestScope,
        thread_id: &str,
        artifact_id: &str,
    ) -> Result<String, CoreError> {
        ensure_safe_scope_segment("thread_id", thread_id).map_err(ks_err)?;
        ensure_safe_scope_segment("artifact_id", artifact_id).map_err(ks_err)?;
        Ok(self.scoped_key(
            scope,
            &["threads", &format!("{thread_id}.{artifact_id}.json")],
        ))
    }
    fn logs_prefix(&self, scope: &RequestScope) -> String {
        self.scoped_prefix(scope, &["logs"])
    }
    fn thread_log_key(&self, scope: &RequestScope, thread_id: &str) -> Result<String, CoreError> {
        ensure_safe_scope_segment("thread_id", thread_id).map_err(ks_err)?;
        Ok(self.scoped_key(scope, &["logs", &format!("{thread_id}.log")]))
    }
}

#[derive(Clone, Debug)]
pub struct DefaultKeyspace {
    pub bucket: String,
}

impl DefaultKeyspace {
    pub fn new(bucket: String) -> Self {
        Self { bucket }
    }
}

impl Keyspace for DefaultKeyspace {
    fn scoped_key(&self, scope: &RequestScope, segments: &[&str]) -> String {
        let mut path = format!("{}/{}/{}", scope.tenant, scope.workspace, scope.project_id);
        for seg in segments {
            path.push('/');
            path.push_str(seg);
        }
        path
    }

    fn scoped_prefix(&self, scope: &RequestScope, segments: &[&str]) -> String {
        let mut path = self.scoped_key(scope, segments);
        if !path.ends_with('/') {
            path.push('/');
        }
        path
    }
}

/// Local filesystem keyspace.
///
/// Same key layout as DefaultKeyspace.
#[derive(Clone, Debug)]
pub struct LocalKeyspace;

impl LocalKeyspace {
    pub fn new(_root_dir: String) -> Self {
        Self
    }
}

impl Keyspace for LocalKeyspace {
    fn scoped_key(&self, scope: &RequestScope, segments: &[&str]) -> String {
        DefaultKeyspace {
            bucket: String::new(),
        }
        .scoped_key(scope, segments)
    }

    fn scoped_prefix(&self, scope: &RequestScope, segments: &[&str]) -> String {
        DefaultKeyspace {
            bucket: String::new(),
        }
        .scoped_prefix(scope, segments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyspace_rejects_bad_segments() {
        let ks = DefaultKeyspace::new("b".to_string());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        assert!(ks.thread_key(&scope, "../x").is_err());
        assert!(ks.thread_key(&scope, "a/b").is_err());
        assert!(ks.thread_key(&scope, "").is_err());
        assert!(ks.thread_artifact_key(&scope, "123", "../x").is_err());
        assert!(ks.thread_artifact_key(&scope, "123", "a/b").is_err());
        assert!(ks.thread_artifact_key(&scope, "123", "").is_err());
    }

    #[test]
    fn keyspace_builds_thread_key() {
        let ks = DefaultKeyspace::new("b".to_string());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let k = ks.thread_key(&scope, "123").unwrap();
        assert_eq!(k, "t/w/p/threads/123.json");
    }

    #[test]
    fn keyspace_builds_thread_state_key() {
        let ks = DefaultKeyspace::new("b".to_string());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let k = ks.thread_state_key(&scope, "123").unwrap();
        assert_eq!(k, "t/w/p/state/123/state.json");
    }

    #[test]
    fn keyspace_builds_control_state_key() {
        let ks = DefaultKeyspace::new("b".to_string());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let k = ks.control_state_key(&scope, "123").unwrap();
        assert_eq!(k, "t/w/p/state/123/control.json");
    }

    #[test]
    fn keyspace_builds_thread_log_key() {
        let ks = DefaultKeyspace::new("b".to_string());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let k = ks.thread_log_key(&scope, "123").unwrap();
        assert_eq!(k, "t/w/p/logs/123.log");
    }

    #[test]
    fn keyspace_builds_thread_artifact_key() {
        let ks = DefaultKeyspace::new("b".to_string());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let k = ks.thread_artifact_key(&scope, "123", "control").unwrap();
        assert_eq!(k, "t/w/p/threads/123.control.json");
    }

    #[test]
    fn scoped_key_builds_arbitrary_path() {
        let ks = DefaultKeyspace::new("b".to_string());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        assert_eq!(
            ks.scoped_key(&scope, &["section_a", "mydb.yaml"]),
            "t/w/p/section_a/mydb.yaml"
        );
        assert_eq!(ks.scoped_prefix(&scope, &["section_b"]), "t/w/p/section_b/");
    }
}
