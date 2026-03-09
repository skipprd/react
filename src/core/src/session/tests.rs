use super::*;
use crate::keyspace::{DefaultKeyspace, Keyspace};
use crate::scope::RequestScope;
use crate::storage::StorageAdapter;
use crate::test_support::InMemoryStorageAdapter;
use std::sync::Arc;

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
