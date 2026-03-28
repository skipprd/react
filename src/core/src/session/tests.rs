use super::*;
use crate::error::CoreError;
use crate::keyspace::{DefaultKeyspace, Keyspace};
use crate::scope::RequestScope;
use crate::storage::{ConditionalWriteStatus, StorageAdapter};
use crate::test_support::InMemoryStorageAdapter;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone, Default)]
struct SequencedJsonStorage {
    head_responses: Arc<Mutex<Vec<Result<Option<String>, CoreError>>>>,
    json_value: Arc<Mutex<Option<Value>>>,
}

#[async_trait]
impl StorageAdapter for SequencedJsonStorage {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        self.json_value
            .lock()
            .map_err(|_| CoreError::Storage(format!("get_json('{key}'): lock poisoned")))?
            .clone()
            .ok_or_else(|| CoreError::Storage(format!("get_json('{key}'): not found")))
    }

    async fn put_json(&self, _key: &str, value: &Value) -> Result<(), CoreError> {
        *self
            .json_value
            .lock()
            .map_err(|_| CoreError::Storage("put_json: lock poisoned".to_string()))? =
            Some(value.clone());
        Ok(())
    }

    async fn put_json_if_etag_matches(
        &self,
        key: &str,
        value: &Value,
        expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError> {
        let mut value_guard = self.json_value.lock().map_err(|_| {
            CoreError::Storage("put_json_if_etag_matches: lock poisoned".to_string())
        })?;
        let current_etag = value_guard.as_ref().map(|_| "stable".to_string());
        let expected = expected_etag.map(str::to_string);
        if current_etag != expected {
            return Ok(ConditionalWriteStatus::Conflict { current_etag });
        }
        *value_guard = Some(value.clone());
        if key.is_empty() {
            return Err(CoreError::Storage(
                "put_json_if_etag_matches: empty key".to_string(),
            ));
        }
        Ok(ConditionalWriteStatus::Written)
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::Storage(format!(
            "get_bytes('{key}'): unsupported in test"
        )))
    }

    async fn put_bytes(
        &self,
        key: &str,
        _bytes: &[u8],
        _content_type: &str,
    ) -> Result<(), CoreError> {
        Err(CoreError::Storage(format!(
            "put_bytes('{key}'): unsupported in test"
        )))
    }

    async fn delete_object(&self, _key: &str) -> Result<(), CoreError> {
        Ok(())
    }

    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        let mut guard = self
            .head_responses
            .lock()
            .map_err(|_| CoreError::Storage(format!("head_etag('{key}'): lock poisoned")))?;
        if guard.is_empty() {
            return Ok(self
                .json_value
                .lock()
                .map_err(|_| {
                    CoreError::Storage(format!("head_etag('{key}'): json lock poisoned"))
                })?
                .as_ref()
                .map(|_| "stable".to_string()));
        }
        guard.remove(0)
    }

    async fn list_prefix(&self, _prefix: &str) -> Result<Vec<String>, CoreError> {
        Ok(vec![])
    }
}

#[test]
fn tool_observation_normalizes_legacy_error_field_into_errors_array() {
    let obs = ToolObservation::normalize(serde_json::json!({"ok": false, "error": "boom"}));
    assert!(!obs.ok);
    assert_eq!(obs.errors, vec!["boom".to_string()]);
    assert!(!obs.extra.contains_key("error"));
}

#[test]
fn thread_log_v1_missing_schema_version_fails_to_deserialize() {
    let v1 = serde_json::json!({
        "steps": [{
            "action": "user",
            "args": {"text": "hi"},
            "observation": {"ok": true},
            "ts": "t",
            "agent": "ask"
        }],
        "result": null
    });
    assert!(serde_json::from_value::<ThreadLog>(v1).is_err());
}

#[test]
fn unknown_fields_in_thread_step_fail_to_deserialize() {
    let bad = serde_json::json!({
        "schema_version": THREAD_SCHEMA_VERSION,
        "steps": [{
            "type": "user",
            "text": "hi",
            "observation": { "ok": true, "errors": [], "warnings": [] },
            "ts": "t",
            "agent": "ask",
            "unexpected": 123
        }],
        "result": null,
        "title": null,
        "title_locked": false
    });
    assert!(serde_json::from_value::<ThreadLog>(bad).is_err());
}

#[tokio::test]
async fn append_step_if_new_deduplicates_interrupt_and_complete() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);
    let tid = "tid-dedupe";

    let step_interrupt = ThreadStep::Interrupt {
        kind: "await_user".to_string(),
        prompt: "same?".to_string(),
        observation: Observation::ok(),
        ts: "2026-01-01T00:00:00Z".to_string(),
        agent: "agent".to_string(),
    };
    store.append_step_if_new(tid, step_interrupt.clone()).await;
    store.append_step_if_new(tid, step_interrupt).await;

    let step_complete = ThreadStep::Complete {
        kind: "generic".to_string(),
        payload: serde_json::json!({"x":1}),
        display: Some("ok".to_string()),
        observation: Observation::ok(),
        ts: "2026-01-01T00:00:01Z".to_string(),
        agent: "agent".to_string(),
    };
    store.append_step_if_new(tid, step_complete.clone()).await;
    store.append_step_if_new(tid, step_complete).await;

    let log = store.get(tid).await.expect("log");
    assert_eq!(log.steps.len(), 2);
}

#[tokio::test]
async fn control_state_store_round_trips() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage, scope, keyspace);
    let tid = "tid-control-envelope";

    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct TestState {
        schema_version: u32,
        mode: String,
    }

    let state = TestState {
        schema_version: 1,
        mode: "mutate".to_string(),
    };
    control.save(tid, "test_suite", &state).await.expect("save");
    let loaded: Option<TestState> = control.load(tid, "test_suite").await.expect("load");
    assert_eq!(loaded, Some(state));
}

#[tokio::test]
async fn control_state_store_returns_none_for_wrong_suite() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage, scope, keyspace);
    let tid = "tid-control-wrong-suite";

    control
        .save(tid, "suite_a", &serde_json::json!({"x":1}))
        .await
        .expect("save");
    let loaded: Option<serde_json::Value> = control.load(tid, "suite_b").await.expect("load");
    assert_eq!(loaded, None);
}

#[tokio::test]
async fn append_step_propagates_read_failure_instead_of_defaulting() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(SequencedJsonStorage {
        head_responses: Arc::new(Mutex::new(vec![Err(CoreError::Storage(
            "boom".to_string(),
        ))])),
        json_value: Arc::new(Mutex::new(None)),
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);

    let err = store
        .append_step(
            "tid-read-failure",
            ThreadStep::User {
                text: "hi".to_string(),
                observation: Observation::ok(),
                ts: "t".to_string(),
                agent: "agent".to_string(),
            },
        )
        .await
        .expect_err("read failure must not be treated as empty thread");
    assert!(err.to_string().contains("boom"));
}

#[tokio::test]
async fn control_state_load_propagates_read_failure_instead_of_none() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(SequencedJsonStorage {
        head_responses: Arc::new(Mutex::new(vec![Err(CoreError::Storage(
            "boom".to_string(),
        ))])),
        json_value: Arc::new(Mutex::new(None)),
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage, scope, keyspace);

    let err = control
        .load::<serde_json::Value>("tid-control-read-failure", "suite")
        .await
        .expect_err("read failure must not be treated as missing control state");
    assert!(err.to_string().contains("boom"));
}

#[tokio::test]
async fn append_step_returns_conflict_when_etag_changes_during_write() {
    let existing = serde_json::to_value(ThreadLog::default()).expect("thread log to json");
    let storage: Arc<dyn StorageAdapter> = Arc::new(SequencedJsonStorage {
        head_responses: Arc::new(Mutex::new(vec![
            Ok(Some("v1".to_string())),
            Ok(Some("v2".to_string())),
        ])),
        json_value: Arc::new(Mutex::new(Some(existing))),
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);

    let err = store
        .append_step(
            "tid-thread-conflict",
            ThreadStep::User {
                text: "hi".to_string(),
                observation: Observation::ok(),
                ts: "t".to_string(),
                agent: "agent".to_string(),
            },
        )
        .await
        .expect_err("etag mismatch must surface as write conflict");
    assert!(err.to_string().contains("write conflict"));
}

#[tokio::test]
async fn control_state_mutate_returns_conflict_when_etag_changes_during_write() {
    let envelope = serde_json::json!({
        "schema_version": CONTROL_STATE_ENVELOPE_SCHEMA_VERSION,
        "suite_id": "suite",
        "payload": { "count": 1 }
    });
    let storage: Arc<dyn StorageAdapter> = Arc::new(SequencedJsonStorage {
        head_responses: Arc::new(Mutex::new(vec![
            Ok(Some("v1".to_string())),
            Ok(Some("v2".to_string())),
        ])),
        json_value: Arc::new(Mutex::new(Some(envelope))),
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage, scope, keyspace);

    let err = control
        .mutate::<serde_json::Value>("tid-control-conflict", "suite", |current| {
            let mut next = current.unwrap_or_else(|| serde_json::json!({}));
            next["count"] = serde_json::json!(2);
            Ok(next)
        })
        .await
        .expect_err("etag mismatch must surface as write conflict");
    assert!(err.to_string().contains("write conflict"));
}

#[tokio::test]
async fn run_observed_appends_failed_tool_end_when_operation_panics() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);
    let tid = "tid-observed-panic";

    let task_store = store.clone();
    let join = tokio::spawn(async move {
        let _ = task_store
            .run_observed(
                tid,
                ToolStepMeta {
                    agent: "agent".to_string(),
                    phase: "test_phase".to_string(),
                    name: "panic_tool".to_string(),
                    clean_name: "Panic Tool".to_string(),
                    args: serde_json::json!({}),
                    ctx: None,
                },
                || async move { panic!("boom") },
                |_raw: &Value| Ok(serde_json::json!({"ok": true})),
            )
            .await;
    });
    let join_err = join.await.expect_err("panic should surface as join error");
    assert!(join_err.is_panic());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let log = store.get(tid).await.expect("log");
    assert_eq!(
        log.steps.len(),
        2,
        "panic finalizer should close the tool span"
    );
    assert!(matches!(log.steps[0], ThreadStep::ToolStart { .. }));
    match &log.steps[1] {
        ThreadStep::ToolEnd {
            status,
            observation,
            ..
        } => {
            assert_eq!(*status, ToolStepStatus::Failed);
            assert!(
                observation.errors.iter().any(|e| e.contains("panicked")),
                "expected panic marker in observation errors: {:?}",
                observation.errors
            );
        }
        other => panic!("expected tool end, got {other:?}"),
    }
}
