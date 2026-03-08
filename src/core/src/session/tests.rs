use std::collections::BTreeMap;
use super::*;
use crate::keyspace::DefaultKeyspace;
use crate::test_support::InMemoryStorageAdapter;

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

#[test]
fn orphan_tool_end_does_not_emit_timeline_event() {
    let log = ThreadLog {
        schema_version: THREAD_SCHEMA_VERSION,
        steps: vec![ThreadStep::ToolEnd {
            tool_id: "orphan".to_string(),
            name: "run_sql".to_string(),
            clean_name: "Run SQL".to_string(),
            args: serde_json::json!({}),
            status: ToolStepStatus::Ok,
            payload: None,
            ctx: None,
            observation: ToolObservation::normalize(serde_json::json!({"ok": true})),
            ts: "t".to_string(),
            agent: "ask".to_string(),
        }],
        result: None,
        title: None,
        title_locked: false,
    };
    let events = build_thread_events_from_log(&log, 200);
    assert!(
        events.is_empty(),
        "orphan tool_end should not appear in timeline events"
    );
}

#[test]
fn orphan_tool_start_does_not_emit_timeline_event() {
    let log = ThreadLog {
        schema_version: THREAD_SCHEMA_VERSION,
        steps: vec![ThreadStep::ToolStart {
            tool_id: "orphan".to_string(),
            name: "run_sql".to_string(),
            clean_name: "Run SQL".to_string(),
            args: serde_json::json!({}),
            status: ToolStepStatus::Running,
            payload: None,
            ctx: None,
            ts: "t".to_string(),
            agent: "ask".to_string(),
        }],
        result: None,
        title: None,
        title_locked: false,
    };
    let events = build_thread_events_from_log(&log, 200);
    assert!(
        events.is_empty(),
        "orphan tool_start should not appear in timeline events"
    );
}

#[test]
fn tool_timeline_events_embed_execution_ctx() {
    let ctx = ExecutionContext {
        data: BTreeMap::from([
            ("suite".to_string(), serde_json::json!("suite_x")),
            ("flow".to_string(), serde_json::json!("plan_a")),
            ("task_id".to_string(), serde_json::json!("task1")),
        ]),
        ..Default::default()
    };
    let log = ThreadLog {
        schema_version: THREAD_SCHEMA_VERSION,
        steps: vec![
            ThreadStep::ToolStart {
                tool_id: "t1".to_string(),
                name: "file".to_string(),
                clean_name: "Read file".to_string(),
                args: serde_json::json!({"op":"get"}),
                status: ToolStepStatus::Running,
                payload: None,
                ctx: Some(ctx.clone()),
                ts: "t".to_string(),
                agent: "agent".to_string(),
            },
            ThreadStep::ToolEnd {
                tool_id: "t1".to_string(),
                name: "file".to_string(),
                clean_name: "Read file".to_string(),
                args: serde_json::json!({"op":"get"}),
                status: ToolStepStatus::Ok,
                payload: None,
                ctx: Some(ctx.clone()),
                observation: ToolObservation::normalize(serde_json::json!({"ok": true})),
                ts: "t".to_string(),
                agent: "agent".to_string(),
            },
        ],
        result: None,
        title: None,
        title_locked: false,
    };
    let events = build_thread_events_from_log(&log, 200);
    assert!(
        events
            .iter()
            .any(|e| e.event_kind == ThreadEventKind::ToolStart),
        "expected tool_start event"
    );
    let ev = events
        .iter()
        .find(|e| e.event_kind == ThreadEventKind::ToolStart)
        .and_then(|e| e.ctx.as_ref())
        .and_then(|c| c.data.get("flow"))
        .and_then(|v| v.as_str());
    assert_eq!(ev, Some("plan_a"));
}

#[test]
fn thread_log_round_trips_with_llm_call_step() {
    let log = ThreadLog {
        schema_version: THREAD_SCHEMA_VERSION,
        steps: vec![ThreadStep::LlmCall {
            call_id: 1,
            model: "unknown".to_string(),
            phase: "test".to_string(),
            prompt_hash: "p".to_string(),
            parts: vec![serde_json::json!({"name":"system","hash":"h","text":"hello"})],
            part_hashes: BTreeMap::from([("system".to_string(), "h".to_string())]),
            response_hash: "r".to_string(),
            response_text: Some("ok".to_string()),
            observation: Observation::ok(),
            ts: "t".to_string(),
            agent: "a".to_string(),
        }],
        result: None,
        title: None,
        title_locked: false,
    };
    let v = serde_json::to_value(&log).unwrap();
    let parsed: ThreadLog = serde_json::from_value(v).unwrap();
    assert_eq!(parsed.schema_version, THREAD_SCHEMA_VERSION);
    assert_eq!(parsed.steps.len(), 1);
    match &parsed.steps[0] {
        ThreadStep::LlmCall {
            call_id,
            phase,
            response_text,
            ..
        } => {
            assert_eq!(*call_id, 1);
            assert_eq!(phase, "test");
            assert_eq!(response_text.as_deref(), Some("ok"));
        }
        _ => panic!("expected llm_call step"),
    }
}

#[test]
fn thread_log_serializes_and_deserializes_llm_call_step() {
    let step = ThreadStep::LlmCall {
        call_id: 1,
        model: "m".to_string(),
        phase: "p".to_string(),
        prompt_hash: "ph".to_string(),
        parts: vec![serde_json::json!({"name":"system","hash":"h","text":"x"})],
        part_hashes: BTreeMap::from([("system".to_string(), "h".to_string())]),
        response_hash: "rh".to_string(),
        response_text: Some("resp".to_string()),
        observation: Observation::ok(),
        ts: "t".to_string(),
        agent: "a".to_string(),
    };
    let log = ThreadLog {
        schema_version: THREAD_SCHEMA_VERSION,
        steps: vec![step],
        result: None,
        title: None,
        title_locked: false,
    };
    let v = serde_json::to_value(&log).unwrap();
    let parsed: ThreadLog = serde_json::from_value(v).unwrap();
    assert_eq!(parsed.steps.len(), 1);
    match &parsed.steps[0] {
        ThreadStep::LlmCall {
            call_id,
            model,
            phase,
            response_text,
            ..
        } => {
            assert_eq!(*call_id, 1);
            assert_eq!(model, "m");
            assert_eq!(phase, "p");
            assert_eq!(response_text.as_deref(), Some("resp"));
        }
        _ => panic!("expected llm_call"),
    }
}

#[tokio::test]
async fn thread_state_is_materialized_from_appended_steps() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);

    let tid = "tid";
    let t0 = chrono::DateTime::parse_from_rfc3339("2026-01-26T00:00:00Z")
        .unwrap()
        .to_rfc3339();
    let t1 = chrono::DateTime::parse_from_rfc3339("2026-01-26T00:00:05Z")
        .unwrap()
        .to_rfc3339();
    let t2 = chrono::DateTime::parse_from_rfc3339("2026-01-26T00:00:06Z")
        .unwrap()
        .to_rfc3339();

    store
        .append_step(
            tid,
            ThreadStep::Phase {
                phase: "model_plan".to_string(),
                from_phase: None,
                reason_code: None,
                reason_detail: None,
                observation: Observation::ok(),
                ts: t0.clone(),
                agent: "agent".to_string(),
            },
        )
        .await
        .unwrap();

    store
        .append_step(
            tid,
            ThreadStep::Phase {
                phase: "model_author".to_string(),
                from_phase: Some("model_plan".to_string()),
                reason_code: None,
                reason_detail: None,
                observation: Observation::ok(),
                ts: t1.clone(),
                agent: "agent".to_string(),
            },
        )
        .await
        .unwrap();

    store
        .append_step(
            tid,
            ThreadStep::ToolStart {
                tool_id: "t1".to_string(),
                name: "validate_tool".to_string(),
                clean_name: "Validate Tool".to_string(),
                args: serde_json::json!({"build": true}),
                status: ToolStepStatus::Running,
                payload: None,
                ctx: None,
                ts: t2.clone(),
                agent: "agent".to_string(),
            },
        )
        .await
        .unwrap();
    store
        .append_step(
            tid,
            ThreadStep::ToolEnd {
                tool_id: "t1".to_string(),
                name: "validate_tool".to_string(),
                clean_name: "Validate Tool".to_string(),
                args: serde_json::json!({"build": true}),
                status: ToolStepStatus::Failed,
                payload: None,
                ctx: None,
                observation: ToolObservation::normalize(serde_json::json!({"ok": false, "errors": ["boom"], "logs": {"run_or_build": {"stdout": "line 1:1 error"}}})),
                ts: t2.clone(),
                agent: "agent".to_string(),
            },
        )
        .await
        .unwrap();

    let st = store.get_thread_state(tid).await.unwrap();
    assert_eq!(st.thread_state_schema_version, THREAD_STATE_SCHEMA_VERSION);
    assert_eq!(st.thread_id, tid);
    assert_eq!(st.current_phase.as_deref(), Some("model_author"));
    let ph = st
        .items
        .get("phase:model_plan")
        .expect("phase:model_plan present");
    assert_eq!(ph.runtime_ms, Some(5_000));
    assert!(st.total_runtime_ms >= 5_000);
    assert!(
        st.items.get("tool:t1").is_none(),
        "tool:* keys must not be materialized into thread_state.items"
    );
}

#[tokio::test]
async fn complete_closes_out_current_phase_as_completed() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);

    let tid = "tid2";
    let t0 = chrono::DateTime::parse_from_rfc3339("2026-01-26T00:00:00Z")
        .unwrap()
        .to_rfc3339();
    let t1 = chrono::DateTime::parse_from_rfc3339("2026-01-26T00:00:01Z")
        .unwrap()
        .to_rfc3339();

    store
        .append_step(
            tid,
            ThreadStep::Phase {
                phase: "done".to_string(),
                from_phase: None,
                reason_code: None,
                reason_detail: None,
                observation: Observation::ok(),
                ts: t0.clone(),
                agent: "agent".to_string(),
            },
        )
        .await
        .unwrap();

    store
        .append_step(
            tid,
            ThreadStep::Complete {
                kind: "generic".to_string(),
                payload: serde_json::json!({"text":"ok"}),
                display: Some("ok".to_string()),
                observation: Observation::ok(),
                ts: t1.clone(),
                agent: "agent".to_string(),
            },
        )
        .await
        .unwrap();

    let st = store.get_thread_state(tid).await.unwrap();
    assert_eq!(st.current_phase.as_deref(), Some("done"));
    let ph = st.items.get("phase:done").expect("phase:done present");
    assert_eq!(ph.status, ThreadItemStatus::Ok);
    assert_eq!(ph.finished_at.as_deref(), Some(t1.as_str()));
}

#[tokio::test]
async fn list_returns_only_thread_logs_not_thread_state_snapshots() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());

    let tid = "123";
    store
        .append_step(
            tid,
            ThreadStep::User {
                text: "hi".to_string(),
                observation: Observation::ok(),
                ts: "t".to_string(),
                agent: "ask".to_string(),
            },
        )
        .await
        .unwrap();

    let state_key = keyspace.thread_state_key(&scope, tid).unwrap();
    let st = ThreadLogViewCache {
        thread_state_schema_version: THREAD_STATE_SCHEMA_VERSION,
        thread_id: tid.to_string(),
        ..Default::default()
    };
    storage
        .put_json(&state_key, &serde_json::to_value(&st).unwrap())
        .await
        .unwrap();

    let ids = store.list().await;
    assert_eq!(ids, vec![tid.to_string()]);
}

#[tokio::test]
async fn get_thread_state_requires_explicit_state_file() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
    let tid = "tid-no-state";

    let key = keyspace.thread_key(&scope, tid).unwrap();
    let log = ThreadLog {
        schema_version: THREAD_SCHEMA_VERSION,
        steps: vec![],
        result: None,
        title: None,
        title_locked: false,
    };
    storage
        .put_json(&key, &serde_json::to_value(log).unwrap())
        .await
        .unwrap();

    let got = store.get_thread_state(tid).await;
    assert!(got.is_err(), "get_thread_state must fail when no explicit state file exists");
}

#[tokio::test]
async fn put_thread_state_merges_non_destructively() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);
    let tid = "tid-merge";

    let base = ThreadLogViewCache {
        thread_state_schema_version: THREAD_STATE_SCHEMA_VERSION,
        thread_id: tid.to_string(),
        suite_id: Some("test_suite".to_string()),
        ..ThreadLogViewCache::default()
    };
    store.put_thread_state(tid, &base).await.unwrap();

    let patch = ThreadLogViewCache {
        thread_state_schema_version: THREAD_STATE_SCHEMA_VERSION,
        thread_id: tid.to_string(),
        current_phase: Some("model_author".to_string()),
        ..ThreadLogViewCache::default()
    };
    store.put_thread_state(tid, &patch).await.unwrap();

    let got = store.get_thread_state(tid).await.unwrap();
    assert_eq!(got.suite_id.as_deref(), Some("test_suite"));
    assert_eq!(got.current_phase.as_deref(), Some("model_author"));
}

#[tokio::test]
async fn put_thread_state_rejects_schema_version_mismatch() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);
    let tid = "tid-schema-check";
    let st = ThreadLogViewCache {
        thread_state_schema_version: THREAD_STATE_SCHEMA_VERSION.saturating_add(1),
        thread_id: tid.to_string(),
        ..ThreadLogViewCache::default()
    };
    let got = store.put_thread_state(tid, &st).await;
    assert!(got.is_err(), "schema mismatches must fail");
}

#[tokio::test]
async fn put_thread_state_rejects_thread_id_mismatch() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);
    let st = ThreadLogViewCache {
        thread_state_schema_version: THREAD_STATE_SCHEMA_VERSION,
        thread_id: "other-thread".to_string(),
        ..ThreadLogViewCache::default()
    };
    let got = store.put_thread_state("tid-id-check", &st).await;
    assert!(got.is_err(), "thread_id mismatches must fail");
}

#[tokio::test]
async fn session_key_build_failures_are_hard_errors() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);
    let bad_tid = "bad/id";

    let append_err = store
        .append_step(
            bad_tid,
            ThreadStep::User {
                text: "hi".to_string(),
                observation: Observation::ok(),
                ts: "t".to_string(),
                agent: "ask".to_string(),
            },
        )
        .await
        .expect_err("invalid thread ids must fail key construction");
    assert!(append_err.to_string().contains("failed to build thread key"));

    let state = ThreadLogViewCache {
        thread_state_schema_version: THREAD_STATE_SCHEMA_VERSION,
        thread_id: bad_tid.to_string(),
        ..ThreadLogViewCache::default()
    };
    let write_err = store
        .put_thread_state_replace(bad_tid, &state)
        .await
        .expect_err("invalid thread ids must fail state key construction");
    assert!(write_err.to_string().contains("failed to build thread state key"));
}

#[tokio::test]
async fn control_state_store_round_trips() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let control = ControlStateStore::new(storage, scope, keyspace);
    let tid = "tid-control-envelope";

    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct TestState { schema_version: u32, mode: String }

    let state = TestState { schema_version: 1, mode: "mutate".to_string() };
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

    control.save(tid, "suite_a", &serde_json::json!({"x":1})).await.expect("save");
    let loaded: Option<serde_json::Value> = control.load(tid, "suite_b").await.expect("load");
    assert_eq!(loaded, None);
}

#[tokio::test]
async fn materialization_gap_reset_clears_stale_state_items() {
    let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let store = ThreadStore::new(storage, scope, keyspace);
    let tid = "tid-gap-reset";

    let mut stale = ThreadLogViewCache {
        thread_state_schema_version: THREAD_STATE_SCHEMA_VERSION,
        thread_id: tid.to_string(),
        last_materialized_step_count: 1,
        ..ThreadLogViewCache::default()
    };
    stale.items.insert(
        "phase:stale".to_string(),
        ThreadItemState {
            kind: ThreadItemKind::Phase,
            status: ThreadItemStatus::Ok,
            started_at: Some("2026-01-01T00:00:00Z".to_string()),
            finished_at: Some("2026-01-01T00:00:01Z".to_string()),
            runtime_ms: Some(1_000),
            last_error: None,
            outputs: None,
        },
    );
    store
        .put_thread_state_replace(tid, &stale)
        .await
        .expect("seed stale state");

    let step = ThreadStep::User {
        text: "next".to_string(),
        observation: Observation::ok(),
        ts: "2026-01-01T00:00:02Z".to_string(),
        agent: "test".to_string(),
    };
    store
        .materialize_thread_state_incremental(tid, 3, &step)
        .await
        .expect("materialize reset");

    let got = store.get_thread_state(tid).await.expect("load reset state");
    assert!(
        !got.items.contains_key("phase:stale"),
        "reset path must fully replace stale items"
    );
    assert_eq!(got.last_materialized_step_count, 3);
}
