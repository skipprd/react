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
                .map_err(|_| CoreError::Storage(format!("head_etag('{key}'): json lock poisoned")))?
                .as_ref()
                .map(|_| "stable".to_string()));
        }
        guard.remove(0)
    }

    async fn list_prefix(&self, _prefix: &str) -> Result<Vec<String>, CoreError> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct FlakyJsonStorage {
    json_value: Arc<Mutex<Option<Value>>>,
    head_failures: Arc<Mutex<usize>>,
    get_json_failures: Arc<Mutex<usize>>,
    put_json_failures: Arc<Mutex<usize>>,
    conditional_write_failures: Arc<Mutex<usize>>,
    head_calls: Arc<Mutex<usize>>,
    get_json_calls: Arc<Mutex<usize>>,
    put_json_calls: Arc<Mutex<usize>>,
    conditional_write_calls: Arc<Mutex<usize>>,
}

impl FlakyJsonStorage {
    fn take_failure(counter: &Mutex<usize>) -> Result<bool, CoreError> {
        let mut guard = counter
            .lock()
            .map_err(|_| CoreError::Storage("flaky storage mutex poisoned".to_string()))?;
        if *guard > 0 {
            *guard -= 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn increment(counter: &Mutex<usize>) -> Result<(), CoreError> {
        let mut guard = counter
            .lock()
            .map_err(|_| CoreError::Storage("flaky storage mutex poisoned".to_string()))?;
        *guard += 1;
        Ok(())
    }

    fn current_json(&self, key: &str) -> Result<Option<Value>, CoreError> {
        self.json_value
            .lock()
            .map_err(|_| CoreError::Storage(format!("json_value('{key}'): lock poisoned")))?
            .clone()
            .ok_or_else(|| CoreError::Storage(format!("json_value('{key}'): not found")))
            .map(Some)
            .or_else(|err| match err {
                CoreError::Storage(msg) if msg.contains("not found") => Ok(None),
                other => Err(other),
            })
    }
}

#[async_trait]
impl StorageAdapter for FlakyJsonStorage {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        Self::increment(&self.get_json_calls)?;
        if Self::take_failure(&self.get_json_failures)? {
            return Err(CoreError::Storage(format!(
                "get_json('{key}'): transient failure"
            )));
        }
        self.json_value
            .lock()
            .map_err(|_| CoreError::Storage(format!("get_json('{key}'): lock poisoned")))?
            .clone()
            .ok_or_else(|| CoreError::Storage(format!("get_json('{key}'): not found")))
    }

    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
        Self::increment(&self.put_json_calls)?;
        if Self::take_failure(&self.put_json_failures)? {
            return Err(CoreError::Storage(format!(
                "put_json('{key}'): transient failure"
            )));
        }
        *self
            .json_value
            .lock()
            .map_err(|_| CoreError::Storage(format!("put_json('{key}'): lock poisoned")))? =
            Some(value.clone());
        Ok(())
    }

    async fn put_json_if_etag_matches(
        &self,
        key: &str,
        value: &Value,
        expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError> {
        Self::increment(&self.conditional_write_calls)?;
        if Self::take_failure(&self.conditional_write_failures)? {
            return Err(CoreError::Storage(format!(
                "put_json_if_etag_matches('{key}'): transient failure"
            )));
        }
        let mut value_guard = self.json_value.lock().map_err(|_| {
            CoreError::Storage(format!("put_json_if_etag_matches('{key}'): lock poisoned"))
        })?;
        let current_etag = value_guard.as_ref().map(|_| "stable".to_string());
        let expected = expected_etag.map(str::to_string);
        if current_etag != expected {
            return Ok(ConditionalWriteStatus::Conflict { current_etag });
        }
        *value_guard = Some(value.clone());
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
        Self::increment(&self.head_calls)?;
        if Self::take_failure(&self.head_failures)? {
            return Err(CoreError::Storage(format!(
                "head_etag('{key}'): transient failure"
            )));
        }
        Ok(self.current_json(key)?.map(|_| "stable".to_string()))
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
async fn append_complete_step_updates_thread_result() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);
    let tid = "tid-complete-result";

    store
        .append_step(
            tid,
            ThreadStep::Complete {
                kind: "data_engineer_terminal".to_string(),
                payload: serde_json::json!({"status":"failed","phase":"model_plan"}),
                display: Some("terminal failure".to_string()),
                observation: Observation::fail(vec!["terminal failure".to_string()]),
                ts: "2026-01-01T00:00:02Z".to_string(),
                agent: "agent".to_string(),
            },
        )
        .await
        .expect("append complete");

    let log = store.get(tid).await.expect("log");
    let result = log.result.expect("thread result");
    assert_eq!(result.kind, "data_engineer_terminal");
    assert_eq!(result.payload["phase"], "model_plan");
    assert_eq!(result.display.as_deref(), Some("terminal failure"));
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
async fn append_step_retries_transient_head_failure_then_succeeds() {
    let storage = Arc::new(FlakyJsonStorage {
        head_failures: Arc::new(Mutex::new(1)),
        ..Default::default()
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage.clone(), scope, keyspace);

    store
        .append_step(
            "tid-thread-retry-head",
            ThreadStep::User {
                text: "hi".to_string(),
                observation: Observation::ok(),
                ts: "t".to_string(),
                agent: "agent".to_string(),
            },
        )
        .await
        .expect("transient head failure should be retried");

    let log = store.get("tid-thread-retry-head").await.expect("log");
    assert_eq!(log.steps.len(), 1);
    assert_eq!(
        *storage
            .head_calls
            .lock()
            .expect("head_calls mutex poisoned"),
        2
    );
}

#[tokio::test]
async fn thread_store_get_retries_transient_get_json_failure_then_succeeds() {
    let storage = Arc::new(FlakyJsonStorage {
        json_value: Arc::new(Mutex::new(Some(
            serde_json::to_value(ThreadLog::default()).expect("thread log to json"),
        ))),
        get_json_failures: Arc::new(Mutex::new(1)),
        ..Default::default()
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage.clone(), scope, keyspace);

    let log = store
        .get("tid-thread-retry-get-json")
        .await
        .expect("transient get_json failure should be retried");
    assert_eq!(log.steps.len(), 0);
    assert_eq!(
        *storage
            .get_json_calls
            .lock()
            .expect("get_json_calls mutex poisoned"),
        2
    );
}

#[tokio::test]
async fn append_step_retries_transient_conditional_write_failure_then_succeeds() {
    let storage = Arc::new(FlakyJsonStorage {
        conditional_write_failures: Arc::new(Mutex::new(1)),
        ..Default::default()
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage.clone(), scope, keyspace);

    store
        .append_step(
            "tid-thread-retry-write",
            ThreadStep::User {
                text: "hi".to_string(),
                observation: Observation::ok(),
                ts: "t".to_string(),
                agent: "agent".to_string(),
            },
        )
        .await
        .expect("transient conditional write failure should be retried");

    let log = store.get("tid-thread-retry-write").await.expect("log");
    assert_eq!(log.steps.len(), 1);
    assert_eq!(
        *storage
            .conditional_write_calls
            .lock()
            .expect("conditional_write_calls mutex poisoned"),
        2
    );
}

#[tokio::test]
async fn control_state_load_propagates_read_failure_instead_of_none() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(SequencedJsonStorage {
        head_responses: Arc::new(Mutex::new(vec![
            Err(CoreError::Storage("boom-1".to_string())),
            Err(CoreError::Storage("boom-2".to_string())),
            Err(CoreError::Storage("boom-3".to_string())),
            Err(CoreError::Storage("boom-4".to_string())),
        ])),
        json_value: Arc::new(Mutex::new(None)),
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage, scope, keyspace);

    let err = control
        .load::<serde_json::Value>("tid-control-read-failure", "suite")
        .await
        .expect_err("read failure must not be treated as missing control state");
    assert!(err.to_string().contains("boom-4"));
}

#[tokio::test]
async fn control_state_load_retries_transient_head_failure_then_succeeds() {
    let envelope = serde_json::json!({
        "schema_version": CONTROL_STATE_ENVELOPE_SCHEMA_VERSION,
        "suite_id": "suite",
        "payload": { "count": 1 }
    });
    let storage = Arc::new(FlakyJsonStorage {
        json_value: Arc::new(Mutex::new(Some(envelope))),
        head_failures: Arc::new(Mutex::new(1)),
        ..Default::default()
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage.clone(), scope, keyspace);

    let loaded = control
        .load::<serde_json::Value>("tid-control-retry-head", "suite")
        .await
        .expect("transient head failure should be retried");
    assert_eq!(loaded, Some(serde_json::json!({ "count": 1 })));
    assert_eq!(
        *storage
            .head_calls
            .lock()
            .expect("head_calls mutex poisoned"),
        2
    );
}

#[tokio::test]
async fn control_state_load_retries_transient_get_json_failure_then_succeeds() {
    let envelope = serde_json::json!({
        "schema_version": CONTROL_STATE_ENVELOPE_SCHEMA_VERSION,
        "suite_id": "suite",
        "payload": { "count": 1 }
    });
    let storage = Arc::new(FlakyJsonStorage {
        json_value: Arc::new(Mutex::new(Some(envelope))),
        get_json_failures: Arc::new(Mutex::new(1)),
        ..Default::default()
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage.clone(), scope, keyspace);

    let loaded = control
        .load::<serde_json::Value>("tid-control-retry-get-json", "suite")
        .await
        .expect("transient get_json failure should be retried");
    assert_eq!(loaded, Some(serde_json::json!({ "count": 1 })));
    assert_eq!(
        *storage
            .get_json_calls
            .lock()
            .expect("get_json_calls mutex poisoned"),
        2
    );
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
async fn control_state_save_retries_transient_put_json_failure_then_succeeds() {
    let storage = Arc::new(FlakyJsonStorage {
        put_json_failures: Arc::new(Mutex::new(1)),
        ..Default::default()
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage.clone(), scope, keyspace);

    control
        .save(
            "tid-control-save-retry",
            "suite",
            &serde_json::json!({"count": 1}),
        )
        .await
        .expect("transient put_json failure should be retried");

    let loaded = control
        .load::<serde_json::Value>("tid-control-save-retry", "suite")
        .await
        .expect("load should succeed after save")
        .expect("state should exist");
    assert_eq!(loaded, serde_json::json!({ "count": 1 }));
    assert_eq!(
        *storage
            .put_json_calls
            .lock()
            .expect("put_json_calls mutex poisoned"),
        2
    );
}

#[tokio::test]
async fn control_state_mutate_retries_transient_conditional_write_failure_then_succeeds() {
    let envelope = serde_json::json!({
        "schema_version": CONTROL_STATE_ENVELOPE_SCHEMA_VERSION,
        "suite_id": "suite",
        "payload": { "count": 1 }
    });
    let storage = Arc::new(FlakyJsonStorage {
        json_value: Arc::new(Mutex::new(Some(envelope))),
        conditional_write_failures: Arc::new(Mutex::new(1)),
        ..Default::default()
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage.clone(), scope, keyspace);

    let updated = control
        .mutate::<serde_json::Value>("tid-control-conditional-retry", "suite", |current| {
            let mut next = current.unwrap_or_else(|| serde_json::json!({}));
            next["count"] = serde_json::json!(2);
            Ok(next)
        })
        .await
        .expect("transient conditional write failure should be retried");
    assert_eq!(updated, serde_json::json!({ "count": 2 }));
    assert_eq!(
        *storage
            .conditional_write_calls
            .lock()
            .expect("conditional_write_calls mutex poisoned"),
        2
    );
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
