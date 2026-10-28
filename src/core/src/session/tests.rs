use super::*;
use crate::error::CoreError;
use crate::keyspace::{DefaultKeyspace, Keyspace};
use crate::scope::RequestScope;
use crate::storage::StorageError;
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
    always_conflict: bool,
    fail_list_prefix: bool,
    fail_delete_object: bool,
}

#[async_trait]
impl StorageAdapter for SequencedJsonStorage {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        self.json_value
            .lock()
            .map_err(|_| {
                CoreError::from(StorageError::other(format!(
                    "get_json('{key}'): lock poisoned"
                )))
            })?
            .clone()
            .ok_or_else(|| CoreError::from(StorageError::not_found(key)))
    }

    async fn put_json(&self, _key: &str, value: &Value) -> Result<(), CoreError> {
        *self
            .json_value
            .lock()
            .map_err(|_| CoreError::from(StorageError::other("put_json: lock poisoned")))? =
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
            CoreError::from(StorageError::other(
                "put_json_if_etag_matches: lock poisoned",
            ))
        })?;
        if self.always_conflict {
            return Ok(ConditionalWriteStatus::Conflict {
                current_etag: Some("other".to_string()),
            });
        }
        let current_etag = value_guard.as_ref().map(|_| "stable".to_string());
        let expected = expected_etag.map(str::to_string);
        if current_etag != expected {
            return Ok(ConditionalWriteStatus::Conflict { current_etag });
        }
        *value_guard = Some(value.clone());
        if key.is_empty() {
            return Err(CoreError::from(StorageError::other(
                "put_json_if_etag_matches: empty key",
            )));
        }
        Ok(ConditionalWriteStatus::Written {
            etag: "stable".to_string(),
        })
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::from(StorageError::other(format!(
            "get_bytes('{key}'): unsupported in test"
        ))))
    }

    async fn put_bytes(
        &self,
        key: &str,
        _bytes: &[u8],
        _content_type: &str,
    ) -> Result<(), CoreError> {
        Err(CoreError::from(StorageError::other(format!(
            "put_bytes('{key}'): unsupported in test"
        ))))
    }

    async fn delete_object(&self, _key: &str) -> Result<(), CoreError> {
        if self.fail_delete_object {
            return Err(CoreError::from(StorageError::other("delete_object failed")));
        }
        Ok(())
    }

    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        let mut guard = self.head_responses.lock().map_err(|_| {
            CoreError::from(StorageError::other(format!(
                "head_etag('{key}'): lock poisoned"
            )))
        })?;
        if guard.is_empty() {
            return Ok(self
                .json_value
                .lock()
                .map_err(|_| {
                    CoreError::from(StorageError::other(format!(
                        "head_etag('{key}'): json lock poisoned"
                    )))
                })?
                .as_ref()
                .map(|_| "stable".to_string()));
        }
        guard.remove(0)
    }

    async fn list_prefix(&self, _prefix: &str) -> Result<Vec<String>, CoreError> {
        if self.fail_list_prefix {
            return Err(CoreError::from(StorageError::other("list_prefix failed")));
        }
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
            .map_err(|_| CoreError::from(StorageError::other("flaky storage mutex poisoned")))?;
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
            .map_err(|_| CoreError::from(StorageError::other("flaky storage mutex poisoned")))?;
        *guard += 1;
        Ok(())
    }

    fn current_json(&self, _key: &str) -> Result<Option<Value>, CoreError> {
        Ok(self
            .json_value
            .lock()
            .map_err(|_| CoreError::from(StorageError::other("json_value: lock poisoned")))?
            .clone())
    }
}

#[async_trait]
impl StorageAdapter for FlakyJsonStorage {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        Self::increment(&self.get_json_calls)?;
        if Self::take_failure(&self.get_json_failures)? {
            return Err(CoreError::from(StorageError::Throttle(format!(
                "get_json('{key}'): transient failure"
            ))));
        }
        self.json_value
            .lock()
            .map_err(|_| {
                CoreError::from(StorageError::other(format!(
                    "get_json('{key}'): lock poisoned"
                )))
            })?
            .clone()
            .ok_or_else(|| CoreError::from(StorageError::not_found(key)))
    }

    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
        Self::increment(&self.put_json_calls)?;
        if Self::take_failure(&self.put_json_failures)? {
            return Err(CoreError::from(StorageError::Throttle(format!(
                "put_json('{key}'): transient failure"
            ))));
        }
        *self.json_value.lock().map_err(|_| {
            CoreError::from(StorageError::other(format!(
                "put_json('{key}'): lock poisoned"
            )))
        })? = Some(value.clone());
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
            return Err(CoreError::from(StorageError::Throttle(format!(
                "put_json_if_etag_matches('{key}'): transient failure"
            ))));
        }
        let mut value_guard = self.json_value.lock().map_err(|_| {
            CoreError::from(StorageError::other(format!(
                "put_json_if_etag_matches('{key}'): lock poisoned"
            )))
        })?;
        let current_etag = value_guard.as_ref().map(|_| "stable".to_string());
        let expected = expected_etag.map(str::to_string);
        if current_etag != expected {
            return Ok(ConditionalWriteStatus::Conflict { current_etag });
        }
        *value_guard = Some(value.clone());
        Ok(ConditionalWriteStatus::Written {
            etag: "stable".to_string(),
        })
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::from(StorageError::other(format!(
            "get_bytes('{key}'): unsupported in test"
        ))))
    }

    async fn put_bytes(
        &self,
        key: &str,
        _bytes: &[u8],
        _content_type: &str,
    ) -> Result<(), CoreError> {
        Err(CoreError::from(StorageError::other(format!(
            "put_bytes('{key}'): unsupported in test"
        ))))
    }

    async fn delete_object(&self, _key: &str) -> Result<(), CoreError> {
        Ok(())
    }

    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        Self::increment(&self.head_calls)?;
        if Self::take_failure(&self.head_failures)? {
            return Err(CoreError::from(StorageError::Throttle(format!(
                "head_etag('{key}'): transient failure"
            ))));
        }
        Ok(self.current_json(key)?.map(|_| "stable".to_string()))
    }

    async fn list_prefix(&self, _prefix: &str) -> Result<Vec<String>, CoreError> {
        Ok(vec![])
    }
}

#[test]
fn tool_observation_from_failure_sets_errors() {
    let obs = ToolObservation::from_failure("boom");
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
    store
        .append_step_if_new(tid, step_interrupt.clone())
        .await
        .expect("append");
    store
        .append_step_if_new(tid, step_interrupt)
        .await
        .expect("append");

    let step_complete = ThreadStep::Complete {
        kind: "generic".to_string(),
        payload: serde_json::json!({"x":1}),
        display: Some("ok".to_string()),
        observation: Observation::ok(),
        ts: "2026-01-01T00:00:01Z".to_string(),
        agent: "agent".to_string(),
    };
    store
        .append_step_if_new(tid, step_complete.clone())
        .await
        .expect("append");
    store
        .append_step_if_new(tid, step_complete)
        .await
        .expect("append");

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
        head_responses: Arc::new(Mutex::new(vec![
            Err(CoreError::from(StorageError::other("boom-1"))),
            Err(CoreError::from(StorageError::other("boom-2"))),
            Err(CoreError::from(StorageError::other("boom-3"))),
            Err(CoreError::from(StorageError::other("boom-4"))),
            Err(CoreError::from(StorageError::other("boom-5"))),
            Err(CoreError::from(StorageError::other("boom-6"))),
            Err(CoreError::from(StorageError::other("boom-7"))),
        ])),
        json_value: Arc::new(Mutex::new(None)),
        always_conflict: false,
        fail_list_prefix: false,
        fail_delete_object: false,
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
    assert!(err.to_string().contains("boom-1"));
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
    assert!(
        *storage
            .head_calls
            .lock()
            .expect("head_calls mutex poisoned")
            >= 2
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
            Err(CoreError::from(StorageError::other("boom-1"))),
            Err(CoreError::from(StorageError::other("boom-2"))),
            Err(CoreError::from(StorageError::other("boom-3"))),
            Err(CoreError::from(StorageError::other("boom-4"))),
            Err(CoreError::from(StorageError::other("boom-5"))),
            Err(CoreError::from(StorageError::other("boom-6"))),
            Err(CoreError::from(StorageError::other("boom-7"))),
        ])),
        json_value: Arc::new(Mutex::new(None)),
        always_conflict: false,
        fail_list_prefix: false,
        fail_delete_object: false,
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage, scope, keyspace);

    let err = control
        .load::<serde_json::Value>("tid-control-read-failure", "suite")
        .await
        .expect_err("read failure must not be treated as missing control state");
    assert!(err.to_string().contains("boom-1"));
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
        always_conflict: true,
        fail_list_prefix: false,
        fail_delete_object: false,
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
    assert!(matches!(
        err,
        CoreError::AuditUnavailable { retries: 5, .. }
    ));
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
        always_conflict: false,
        fail_list_prefix: false,
        fail_delete_object: false,
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

#[tokio::test]
async fn run_observed_propagates_tool_start_append_failure() {
    let existing = serde_json::to_value(ThreadLog::default()).expect("thread log to json");
    let storage: Arc<dyn StorageAdapter> = Arc::new(SequencedJsonStorage {
        head_responses: Arc::new(Mutex::new(vec![])),
        json_value: Arc::new(Mutex::new(Some(existing))),
        always_conflict: true,
        fail_list_prefix: false,
        fail_delete_object: false,
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);

    let err = store
        .run_observed(
            "tid-observed-conflict",
            ToolStepMeta {
                agent: "agent".to_string(),
                phase: "test_phase".to_string(),
                name: "blocked_tool".to_string(),
                clean_name: "Blocked".to_string(),
                args: serde_json::json!({}),
                ctx: None,
            },
            || async move { Ok(serde_json::json!({"ok": true})) },
            |_raw: &Value| Ok(serde_json::json!({"ok": true})),
        )
        .await
        .expect_err("tool_start append conflict must stop the run");
    assert!(
        err.contains("audit unavailable (tool_start)"),
        "unexpected err: {err}"
    );
}

#[tokio::test]
async fn thread_store_does_not_lose_update_across_instances() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let a = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
    let b = ThreadStore::new(storage, scope, keyspace);
    let tid = "tid-lost-update";

    a.append_step(
        tid,
        ThreadStep::User {
            text: "one".to_string(),
            observation: Observation::ok(),
            ts: "t1".to_string(),
            agent: "agent".to_string(),
        },
    )
    .await
    .expect("first append");
    b.append_step(
        tid,
        ThreadStep::User {
            text: "two".to_string(),
            observation: Observation::ok(),
            ts: "t2".to_string(),
            agent: "agent".to_string(),
        },
    )
    .await
    .expect("second append");

    let log = a.get(tid).await.expect("reload after peer write");
    let texts: Vec<&str> = log
        .steps
        .iter()
        .filter_map(|s| match s {
            ThreadStep::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"one"), "missing first write: {texts:?}");
    assert!(texts.contains(&"two"), "missing second write: {texts:?}");
}

/// After a successful conditional PUT, a peer write must not be hidden by
/// caching (our body, peer etag) from a follow-up HEAD.
#[derive(Clone)]
struct PeerOverwritesAfterPut {
    inner: InMemoryStorageAdapter,
    peer: Value,
}

#[async_trait]
impl StorageAdapter for PeerOverwritesAfterPut {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        self.inner.get_json(key).await
    }
    async fn put_json(&self, key: &str, value: &Value) -> Result<(), CoreError> {
        self.inner.put_json(key, value).await
    }
    async fn put_json_if_etag_matches(
        &self,
        key: &str,
        value: &Value,
        expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError> {
        let status = self
            .inner
            .put_json_if_etag_matches(key, value, expected_etag)
            .await?;
        if matches!(status, ConditionalWriteStatus::Written { .. }) {
            self.inner.put_json(key, &self.peer).await?;
        }
        Ok(status)
    }
    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        self.inner.get_bytes(key).await
    }
    async fn put_bytes(&self, key: &str, bytes: &[u8], ct: &str) -> Result<(), CoreError> {
        self.inner.put_bytes(key, bytes, ct).await
    }
    async fn delete_object(&self, key: &str) -> Result<(), CoreError> {
        self.inner.delete_object(key).await
    }
    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        self.inner.head_etag(key).await
    }
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError> {
        self.inner.list_prefix(prefix).await
    }
}

#[tokio::test]
async fn thread_store_does_not_cache_our_body_under_peer_etag() {
    let peer_log = ThreadLog {
        steps: vec![ThreadStep::User {
            text: "peer".to_string(),
            observation: Observation::ok(),
            ts: "tp".to_string(),
            agent: "agent".to_string(),
        }],
        ..ThreadLog::default()
    };
    let peer = serde_json::to_value(&peer_log).expect("peer log");
    let storage: Arc<dyn StorageAdapter> = Arc::new(PeerOverwritesAfterPut {
        inner: InMemoryStorageAdapter::default(),
        peer,
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);
    let tid = "tid-put-head-race";
    store
        .append_step(
            tid,
            ThreadStep::User {
                text: "ours".to_string(),
                observation: Observation::ok(),
                ts: "to".to_string(),
                agent: "agent".to_string(),
            },
        )
        .await
        .expect("append");
    let log = store.get(tid).await.expect("get after peer overwrite");
    let texts: Vec<&str> = log
        .steps
        .iter()
        .filter_map(|s| match s {
            ThreadStep::User { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        texts.contains(&"peer"),
        "cache must not hide the peer write behind a post-PUT HEAD etag: {texts:?}"
    );
}

#[tokio::test]
async fn thread_store_delete_propagates_list_prefix_failure() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(SequencedJsonStorage {
        head_responses: Arc::new(Mutex::new(vec![])),
        json_value: Arc::new(Mutex::new(None)),
        always_conflict: false,
        fail_list_prefix: true,
        fail_delete_object: false,
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);

    let err = store
        .delete("tid-delete-list")
        .await
        .expect_err("list_prefix failure must surface");
    assert!(
        err.to_string().contains("list_prefix failed"),
        "unexpected err: {err}"
    );
}

#[tokio::test]
async fn control_state_delete_propagates_storage_failure() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(SequencedJsonStorage {
        head_responses: Arc::new(Mutex::new(vec![])),
        json_value: Arc::new(Mutex::new(None)),
        always_conflict: false,
        fail_list_prefix: false,
        fail_delete_object: true,
    });
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage, scope, keyspace);

    let err = control
        .delete("tid-control-delete")
        .await
        .expect_err("delete_object failure must surface");
    assert!(
        err.to_string().contains("delete_object failed"),
        "unexpected err: {err}"
    );
}
