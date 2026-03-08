use super::*;
use crate::wiring::{DefaultKeyspace, RequestScope};
use crate::ws::conn_state::{derive_thread_context, normalize_agent_new, normalize_agent_open};
use crate::ws::history::{build_history, compute_unread_for_log};
use crate::ws::thread_state::ws_thread_state_snapshot_from_core;
use crate::ws::util::DEFAULT_INITIAL_PHASE;
use async_trait::async_trait;
use futures_util::sink::Sink;
use react_core::keyspace::Keyspace;
use react_core::llm::NullModel;
use react_core::session::{
    Observation, ThreadLog, ThreadLogViewCache as CoreThreadLogViewCache, ThreadStep, ThreadStore,
    ToolObservation,
};
use react_core::provider_traits::NullSecretsProvider;
use react_module_storage_memory::InMemoryStorageAdapter;
use serde_json::json;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::time::Duration;

#[test]
fn thread_state_snapshot_maps_ctx_from_core_event_field() {
    let mut core = CoreThreadLogViewCache::default();
    core.thread_state_schema_version = react_core::session::THREAD_STATE_SCHEMA_VERSION;
    core.thread_id = "tid".to_string();
    core.suite_id = Some("suite_x".to_string());
    core.agent_type = Some("agent".to_string());
    core.current_phase = Some(DEFAULT_INITIAL_PHASE.to_string());
    let timeline_events = vec![react_core::session::ThreadEvent {
        step_idx: 0,
        event_kind: react_core::session::ThreadEventKind::ToolStart,
        ts: "t".to_string(),
        ctx: Some({
            let mut ctx = react_core::session::ExecutionContext::default();
            ctx.set("plan_kind", serde_json::Value::String("test_plan_kind".to_string()));
            ctx.set("plan_key", serde_json::Value::String("p1".to_string()));
            ctx.set("workgroup_id", serde_json::Value::String("wg1".to_string()));
            ctx.set("task_id", serde_json::Value::String("task1".to_string()));
            ctx.set("checklist_item_id", serde_json::Value::String("sql_model".to_string()));
            ctx.set("suite", serde_json::json!("suite_x"));
            ctx
        }),
        ..Default::default()
    }];
    let reg = react_core::suite::SuiteRegistry::new();
    let snap = ws_thread_state_snapshot_from_core(&core, &timeline_events, &reg);
    let ctx = snap.events[0].ctx.as_ref().expect("ctx");
    assert_eq!(ctx.plan_kind, Some("test_plan_kind".to_string()));
    assert_eq!(ctx.plan_key.as_deref(), Some("p1"));
    assert_eq!(ctx.workgroup_id.as_deref(), Some("wg1"));
    assert_eq!(ctx.task_id.as_deref(), Some("task1"));
    assert_eq!(ctx.checklist_item_id.as_deref(), Some("sql_model"));
}

#[derive(Clone, Default)]
struct CollectSink {
    out: Arc<Mutex<Vec<String>>>,
}

impl Sink<Message> for CollectSink {
    type Error = std::convert::Infallible;

    fn poll_ready(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        if let Message::Text(s) = item {
            self.out.lock().unwrap().push(s);
        }
        Ok(())
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

struct StubDataEngineerSuite;

#[async_trait]
impl react_core::suite::Suite for StubDataEngineerSuite {
    fn id(&self) -> &'static str {
        "suite_x"
    }

    fn phase_order(&self, _agent_type: &str) -> Vec<String> {
        vec![DEFAULT_INITIAL_PHASE.to_string(), "done".to_string()]
    }

    async fn handle_new(
        &self,
        _thread_id: &str,
        _question: &str,
        _agent_type: &str,
        _ctx: &SuiteCtx,
    ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
        Ok(vec![react_core::suite::FlowFrame::Complete {
            kind: react_core::suite::FlowKind::new("ask"),
            payload: serde_json::json!({"answer":"ok","sql":"SELECT 1"}),
            display: Some("ok".to_string()),
        }])
    }

    async fn handle_open(
        &self,
        _thread_id: &str,
        _question: &str,
        _agent_type: &str,
        _ctx: &SuiteCtx,
    ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
        Ok(vec![react_core::suite::FlowFrame::Complete {
            kind: react_core::suite::FlowKind::new("ask"),
            payload: serde_json::json!({"answer":"ok","sql":"SELECT 1"}),
            display: Some("ok".to_string()),
        }])
    }

    async fn handle_user(
        &self,
        thread_id: &str,
        _text: &str,
        _agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
        // Emit a tool_start/tool_end pair into the durable thread log so WS can stream tool events.
        let store =
            ThreadStore::new(ctx.storage().clone(), ctx.scope().clone(), ctx.keyspace().clone());
        let tool_id = "t1".to_string();
        let _ = store
            .append_step(
                thread_id,
                ThreadStep::ToolStart {
                    tool_id: tool_id.clone(),
                    name: "dbt_files".to_string(),
                    clean_name: "dbt_files patch".to_string(),
                    args: serde_json::json!({"op":"patch"}),
                    status: react_core::session::ToolStepStatus::Running,
                    payload: Some(serde_json::json!({"hint":"starting"})),
                    ctx: None,
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: "agent".to_string(),
                },
            )
            .await;
        let _ = store
            .append_step(
                thread_id,
                ThreadStep::ToolEnd {
                    tool_id: tool_id.clone(),
                    name: "dbt_files".to_string(),
                    clean_name: "dbt_files patch".to_string(),
                    args: serde_json::json!({"op":"patch"}),
                    status: react_core::session::ToolStepStatus::Ok,
                    payload: Some(serde_json::json!({"written_keys": []})),
                    ctx: None,
                    observation: ToolObservation::normalize(serde_json::json!({"ok": true})),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: "agent".to_string(),
                },
            )
            .await;

        // Sleep long enough for WS ticks (tool/state) to emit at least once.
        tokio::time::sleep(Duration::from_millis(650)).await;
        Ok(vec![react_core::suite::FlowFrame::Complete {
            kind: react_core::suite::FlowKind::new("ask"),
            payload: serde_json::json!({"answer":"ok","sql":"SELECT 1"}),
            display: Some("ok".to_string()),
        }])
    }
}

struct StubAwaitApprovalSuite;

#[async_trait]
impl react_core::suite::Suite for StubAwaitApprovalSuite {
    fn id(&self) -> &'static str {
        "suite_x"
    }

    fn phase_order(&self, _agent_type: &str) -> Vec<String> {
        vec![DEFAULT_INITIAL_PHASE.to_string(), "done".to_string()]
    }

    async fn handle_new(
        &self,
        _thread_id: &str,
        _question: &str,
        _agent_type: &str,
        _ctx: &SuiteCtx,
    ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
        Ok(vec![react_core::suite::FlowFrame::Interrupt {
            kind: react_core::suite::FlowKind::new("await_approval"),
            prompt: "approve?".to_string(),
        }])
    }

    async fn handle_open(
        &self,
        _thread_id: &str,
        _question: &str,
        _agent_type: &str,
        _ctx: &SuiteCtx,
    ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
        Ok(vec![react_core::suite::FlowFrame::Interrupt {
            kind: react_core::suite::FlowKind::new("await_approval"),
            prompt: "approve?".to_string(),
        }])
    }

    async fn handle_user(
        &self,
        _thread_id: &str,
        _text: &str,
        _agent_type: &str,
        _ctx: &SuiteCtx,
    ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
        Ok(vec![react_core::suite::FlowFrame::Interrupt {
            kind: react_core::suite::FlowKind::new("await_approval"),
            prompt: "approve?".to_string(),
        }])
    }
}

struct StubBatchLockedSuite;

#[async_trait]
impl react_core::suite::Suite for StubBatchLockedSuite {
    fn id(&self) -> &'static str {
        "suite_x"
    }

    fn phase_order(&self, _agent_type: &str) -> Vec<String> {
        vec!["test_plan_author".to_string()]
    }

    async fn handle_new(
        &self,
        _thread_id: &str,
        _question: &str,
        _agent_type: &str,
        _ctx: &SuiteCtx,
    ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
        Ok(vec![react_core::suite::FlowFrame::Interrupt {
            kind: react_core::suite::FlowKind::new("await_user"),
            prompt: "Plan-batched authoring is locked (test_agent).".to_string(),
        }])
    }

    async fn handle_open(
        &self,
        _thread_id: &str,
        _question: &str,
        _agent_type: &str,
        _ctx: &SuiteCtx,
    ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
        Ok(vec![react_core::suite::FlowFrame::Interrupt {
            kind: react_core::suite::FlowKind::new("await_user"),
            prompt: "Plan-batched authoring is locked (test_agent).".to_string(),
        }])
    }

    async fn handle_user(
        &self,
        _thread_id: &str,
        _text: &str,
        _agent_type: &str,
        _ctx: &SuiteCtx,
    ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
        Ok(vec![react_core::suite::FlowFrame::Interrupt {
            kind: react_core::suite::FlowKind::new("await_user"),
            prompt: "Plan-batched authoring is locked (test_agent).".to_string(),
        }])
    }
}

#[test]
fn normalize_agent_includes_agent_and_review() {
    assert_eq!(normalize_agent_new(api::new_request::AgentType::Agent), "agent");
    assert_eq!(normalize_agent_new(api::new_request::AgentType::Review), "review");
    assert_eq!(normalize_agent_open(api::open_request::AgentType::Agent), "agent");
    assert_eq!(normalize_agent_open(api::open_request::AgentType::Review), "review");
}

#[test]
fn derive_thread_context_ignores_step_agent_labels() {
    let log = ThreadLog {
        steps: vec![
            ThreadStep::SwitchSuite {
                from: None,
                to: "suite_x".to_string(),
                observation: Observation::ok(),
                ts: "t".to_string(),
                agent: "agent".to_string(),
            },
            ThreadStep::SwitchAgent {
                from: None,
                to: "agent".to_string(),
                observation: Observation::ok(),
                ts: "t".to_string(),
                agent: "agent".to_string(),
            },
            // Inner phase/tool steps may record agent labels like "test_agent" — these must NOT
            // override the user-selected agent_type derived from switch_agent.
            ThreadStep::ToolEnd {
                tool_id: "t".to_string(),
                name: "sql_schema".to_string(),
                clean_name: "List tables".to_string(),
                args: json!({}),
                status: react_core::session::ToolStepStatus::Ok,
                payload: None,
                ctx: None,
                observation: ToolObservation::normalize(json!({"ok": true})),
                ts: "t".to_string(),
                agent: "test_agent".to_string(),
            },
        ],
        ..Default::default()
    };
    let (_suite, agent_type) = derive_thread_context(&log);
    assert_eq!(agent_type, "agent");
}

#[test]
fn unread_counts_include_review_response() {
    let log = ThreadLog {
        steps: vec![
            ThreadStep::User {
                text: "hi".to_string(),
                observation: Observation::ok(),
                ts: "t".to_string(),
                agent: "ask".to_string(),
            },
            ThreadStep::ReviewResponse {
                text: "review text".to_string(),
                meta: None,
                observation: Observation::ok(),
                ts: "t".to_string(),
                agent: "agent".to_string(),
            },
            ThreadStep::Complete {
                kind: "generic".to_string(),
                payload: serde_json::json!({ "text": "done" }),
                display: Some("done".to_string()),
                observation: Observation::ok(),
                ts: "t".to_string(),
                agent: "agent".to_string(),
            },
        ],
        ..Default::default()
    };
    // No seen messages yet -> both assistant messages should count as unread.
    let (_max_seq, unread) = compute_unread_for_log(&log, 0);
    assert_eq!(unread, 2);
}

#[tokio::test]
async fn history_includes_review_response_as_assistant_message() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let store = ThreadStore::new(storage, scope, keyspace);

    let tid = "thread1";
    let _ = store
        .append_step(
            tid,
            ThreadStep::User {
                text: "start".to_string(),
                observation: Observation::ok(),
                ts: chrono::Utc::now().to_rfc3339(),
                agent: "ask".to_string(),
            },
        )
        .await;

    let _ = store
        .append_step(
            tid,
            ThreadStep::ReviewResponse {
                text: "review text".to_string(),
                meta: None,
                observation: Observation::ok(),
                ts: chrono::Utc::now().to_rfc3339(),
                agent: "agent".to_string(),
            },
        )
        .await;

    let (msgs, _next) = build_history(&store, tid, None, Some(50)).await.unwrap();
    assert!(msgs.iter().any(
        |m| m.role == m::history_response_messages_inner::Role::Assistant
            && m.content == "review text"
    ));
}

#[tokio::test]
async fn suites_request_requires_cid_and_v() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let suite_ctx = SuiteCtx::new(
        storage,
        Arc::new(NullSecretsProvider::default()),
        Arc::new(NullModel::new()),
        scope,
        keyspace,
    );
    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(react_suite_data_engineer::DataEngineerSuite);
    reg.register(react_suite_kb::KbSuite);
    let reg = Arc::new(reg);
    let mut state = ConnState::new(reg, suite_ctx, None);

    // Missing required fields should fail strict parsing.
    let bad = json!({"type":"suites"}).to_string();
    assert!(handle_message(&bad, &mut state).await.is_err());
}

#[tokio::test]
async fn delete_request_requires_cid_and_v() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let suite_ctx = SuiteCtx::new(
        storage,
        Arc::new(NullSecretsProvider::default()),
        Arc::new(NullModel::new()),
        scope,
        keyspace,
    );
    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(react_suite_data_engineer::DataEngineerSuite);
    reg.register(react_suite_kb::KbSuite);
    let reg = Arc::new(reg);
    let mut state = ConnState::new(reg, suite_ctx, None);

    let bad = json!({"type":"delete","thread_id":"not-a-uuid"}).to_string();
    assert!(handle_message(&bad, &mut state).await.is_err());
}

#[tokio::test]
async fn headless_run_requires_existing_thread_when_thread_id_provided() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let suite_ctx = SuiteCtx::new(
        storage,
        Arc::new(NullSecretsProvider::default()),
        Arc::new(NullModel::new()),
        scope,
        keyspace,
    );
    let missing = uuid::Uuid::new_v4().to_string();
    let hub = EventHub::new(256);
    let err = run_headless_with_hub(
        suite_ctx,
        Some(missing.clone()),
        "suite_x".to_string(),
        "agent".to_string(),
        hub,
        SuiteRegistry::new(),
    )
    .await
    .unwrap_err();
    assert!(err.contains("thread does not exist"));
    assert!(err.contains(&missing));
}

#[tokio::test]
async fn headless_run_exits_with_error_on_ask_user_prompt() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let suite_ctx = SuiteCtx::new(
        storage,
        Arc::new(NullSecretsProvider::default()),
        Arc::new(NullModel::new()),
        scope,
        keyspace,
    );

    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(StubBatchLockedSuite);
    let reg = Arc::new(reg);
    let mut state = ConnState::new(reg, suite_ctx, None);

    let msg = json!({"v":1,"type":"new","cid":"headless","suiteId":"suite_x","agentType":"agent","question":"go"});
    let mut sink = CollectSink::default();
    let err = process_new(&msg, &mut state, &mut sink).await.unwrap_err();
    assert!(
        err.to_ascii_lowercase()
            .contains("ask_user_not_supported_in_headless")
    );
}

#[tokio::test]
async fn plans_request_returns_latest_plan_and_model_when_present() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let suite_ctx = SuiteCtx::new(
        storage.clone(),
        Arc::new(NullSecretsProvider::default()),
        Arc::new(NullModel::new()),
        scope.clone(),
        keyspace.clone(),
    );
    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(react_suite_data_engineer::DataEngineerSuite);
    reg.register(react_suite_kb::KbSuite);
    let reg = Arc::new(reg);
    let mut state = ConnState::new(reg, suite_ctx.clone(), None);

    let thread_id = uuid::Uuid::new_v4().to_string();
    let base = keyspace
        .threads_prefix(&scope)
        .trim_end_matches("/threads")
        .trim_end_matches('/')
        .to_string();

    let test_plan_key = format!("{}/plans/{}/20260126T000000Z_cleanse.json", base, thread_id);
    let test_plan = serde_json::json!({
        "plan_key": test_plan_key,
        "status": "approved",
        "project_snapshot": {},
        "tasks": [],
        "batches": [],
        "work_groups": [],
        "mutations": [],
        "progress": {},
    });
    suite_ctx
        .storage()
        .put_bytes(
            &test_plan_key,
            &serde_json::to_vec_pretty(&test_plan).unwrap(),
            "application/json",
        )
        .await
        .unwrap();

    let model_key = format!("{}/plans/{}/20260126T000000Z_model.json", base, thread_id);
    let model = serde_json::json!({
        "plan_key": model_key,
        "status": "approved",
        "project_snapshot": {},
        "tasks": [],
        "batches": [],
        "work_groups": [],
        "mutations": [],
        "progress": {},
    });
    suite_ctx
        .storage()
        .put_bytes(
            &model_key,
            &serde_json::to_vec_pretty(&model).unwrap(),
            "application/json",
        )
        .await
        .unwrap();

    let msg = json!({"v":1,"type":"plans","cid":"c1","thread_id":thread_id}).to_string();
    let frames = handle_message(&msg, &mut state).await.unwrap();
    assert_eq!(frames.len(), 1);
    let resp: api::PlansResponse = serde_json::from_str(&frames[0]).unwrap();
    assert_eq!(resp.for_cid.as_deref(), Some("c1"));
    assert_eq!(resp.plans.len(), 2);
}

#[tokio::test]
async fn plans_request_includes_checklist_and_omits_notes() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let suite_ctx = SuiteCtx::new(
        storage.clone(),
        Arc::new(NullSecretsProvider::default()),
        Arc::new(NullModel::new()),
        scope.clone(),
        keyspace.clone(),
    );
    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(react_suite_data_engineer::DataEngineerSuite);
    reg.register(react_suite_kb::KbSuite);
    let reg = Arc::new(reg);
    let mut state = ConnState::new(reg, suite_ctx.clone(), None);

    let thread_id = uuid::Uuid::new_v4().to_string();
    let base = keyspace
        .threads_prefix(&scope)
        .trim_end_matches("/threads")
        .trim_end_matches('/')
        .to_string();

    let test_plan_key = format!("{}/plans/{}/20260126T000000Z_cleanse.json", base, thread_id);
    let test_plan = serde_json::json!({
        "plan_key": test_plan_key,
        "status": "approved",
        "project_snapshot": {},
        "tasks": [{
            "dataset_id": "AwsDataCatalog.test_raw.raw_orders",
            "expected_model_path": "models/staging/stg_test_raw_raw_orders.sql",
            "invariants": [],
            "implementation_spec": {
                "spec_version": 1,
                "row_preserving": true,
                "output_fields": [{
                    "name": "order_id",
                    "kind": "raw",
                    "source_columns": [],
                    "expression": "order_id",
                    "data_type": null,
                    "nullable": false,
                    "description": null
                }],
                "prohibited_ops": []
            },
            "status": "pending",
            "checklist": [{
                "checklist_item_id": "sql_model",
                "label": "Author staging SQL",
                "details": null,
                "status": "pending",
                "origin": "initial",
                "evidence": []
            }]
        }],
        "batches": [["AwsDataCatalog.test_raw.raw_orders"]],
        "work_groups": [],
        "mutations": [],
        "progress": {},
    });
    suite_ctx
        .storage()
        .put_bytes(
            &test_plan_key,
            &serde_json::to_vec_pretty(&test_plan).unwrap(),
            "application/json",
        )
        .await
        .unwrap();

    let msg = json!({"v":1,"type":"plans","cid":"c1","thread_id":thread_id}).to_string();
    let frames = handle_message(&msg, &mut state).await.unwrap();
    assert_eq!(frames.len(), 1);

    let v: serde_json::Value = serde_json::from_str(&frames[0]).unwrap();
    // planKind "cleanse" comes from data_engineer suite's load_ws_plans (files *_cleanse.json)
    let test_plan_snap = v
        .get("plans")
        .and_then(|x| x.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|p| p.get("planKind").and_then(|k| k.as_str()) == Some("cleanse"))
        })
        .expect("test plan");
    let tasks = test_plan_snap
        .get("tasks")
        .and_then(|x| x.as_array())
        .unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(
        tasks[0].get("taskKind").and_then(|x| x.as_str()),
        Some("cleanse")
    );
    let cl = tasks[0]
        .get("checklist")
        .and_then(|x| x.as_array())
        .unwrap();
    assert_eq!(
        cl[0].get("checklistItemId").and_then(|x| x.as_str()),
        Some("sql_model")
    );
    assert!(tasks[0].get("notes").is_none());
}

#[tokio::test]
async fn plans_request_surfaces_parse_error_snapshot_for_corrupt_plan_json() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let suite_ctx = SuiteCtx::new(
        storage.clone(),
        Arc::new(NullSecretsProvider::default()),
        Arc::new(NullModel::new()),
        scope.clone(),
        keyspace.clone(),
    );
    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(react_suite_data_engineer::DataEngineerSuite);
    reg.register(react_suite_kb::KbSuite);
    let reg = Arc::new(reg);
    let mut state = ConnState::new(reg, suite_ctx.clone(), None);

    let thread_id = uuid::Uuid::new_v4().to_string();
    let base = keyspace
        .threads_prefix(&scope)
        .trim_end_matches("/threads")
        .trim_end_matches('/')
        .to_string();
    let test_plan_key = format!("{}/plans/{}/20260126T000000Z_cleanse.json", base, thread_id);
    suite_ctx
        .storage()
        .put_bytes(&test_plan_key, b"{not valid json", "application/json")
        .await
        .unwrap();

    let msg = json!({"v":1,"type":"plans","cid":"c1","thread_id":thread_id}).to_string();
    let frames = handle_message(&msg, &mut state).await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&frames[0]).unwrap();
    // planKind "cleanse" comes from data_engineer suite's load_ws_plans (files *_cleanse.json)
    let test_plan_snap = v
        .get("plans")
        .and_then(|x| x.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|p| p.get("planKind").and_then(|k| k.as_str()) == Some("cleanse"))
        })
        .expect("test plan");
    assert_eq!(
        test_plan_snap.get("status").and_then(|x| x.as_str()),
        Some("cancelled")
    );
    let tasks = test_plan_snap
        .get("tasks")
        .and_then(|x| x.as_array())
        .unwrap();
    let cl = tasks[0]
        .get("checklist")
        .and_then(|x| x.as_array())
        .unwrap();
    assert_eq!(
        cl[0].get("checklistItemId").and_then(|x| x.as_str()),
        Some("parse_error")
    );
    let ps = test_plan_snap
        .get("projectSnapshot")
        .and_then(|x| x.as_object())
        .expect("projectSnapshot");
    assert!(ps.get("parse_error").is_some());
}

#[tokio::test]
async fn open_emits_thread_state_frame() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let suite_ctx = SuiteCtx::new(
        storage.clone(),
        Arc::new(NullSecretsProvider::default()),
        Arc::new(NullModel::new()),
        scope.clone(),
        keyspace.clone(),
    );
    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(StubDataEngineerSuite);
    let reg = Arc::new(reg);
    let mut state = ConnState::new(reg, suite_ctx.clone(), None);

    // Seed empty thread log so process_open can load it.
    let thread_id = uuid::Uuid::new_v4().to_string();
    let store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
    // minimal steps so derive_thread_context has something (optional)
    let _ = store
        .append_step(
            &thread_id,
            ThreadStep::SwitchSuite {
                from: None,
                to: "suite_x".to_string(),
                observation: Observation::ok(),
                ts: chrono::Utc::now().to_rfc3339(),
                agent: "agent".to_string(),
            },
        )
        .await;

    let msg = json!({"v":1,"type":"open","cid":"c1","thread_id":thread_id,"suiteId":"suite_x","agentType":"agent","question":"Continue."});
    let mut sink = CollectSink::default();
    process_open(&msg, &mut state, &mut sink).await.unwrap();

    let frames = sink.out.lock().unwrap().clone();
    assert!(frames.iter().any(|s| {
        serde_json::from_str::<serde_json::Value>(s)
            .ok()
            .and_then(|v| {
                v.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t == "thread_state")
            })
            .unwrap_or(false)
    }));
}

#[tokio::test]
async fn open_persists_final_step_in_thread_log() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let suite_ctx = SuiteCtx::new(
        storage.clone(),
        Arc::new(NullSecretsProvider::default()),
        Arc::new(NullModel::new()),
        scope.clone(),
        keyspace.clone(),
    );
    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(StubDataEngineerSuite);
    let reg = Arc::new(reg);
    let mut state = ConnState::new(reg, suite_ctx.clone(), None);

    let thread_id = uuid::Uuid::new_v4().to_string();
    let store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
    let _ = store
        .append_step(
            &thread_id,
            ThreadStep::SwitchSuite {
                from: None,
                to: "suite_x".to_string(),
                observation: Observation::ok(),
                ts: chrono::Utc::now().to_rfc3339(),
                agent: "agent".to_string(),
            },
        )
        .await;

    let msg = json!({"v":1,"type":"open","cid":"c1","thread_id":thread_id,"suiteId":"suite_x","agentType":"agent","question":"Continue."});
    let mut sink = CollectSink::default();
    process_open(&msg, &mut state, &mut sink).await.unwrap();

    let reader = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
    let log = reader.get(&thread_id).await.unwrap();
    assert!(matches!(log.steps.last(), Some(ThreadStep::Complete { .. })));
}

#[tokio::test]
async fn open_persists_await_approval_step_in_thread_log() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let suite_ctx = SuiteCtx::new(
        storage.clone(),
        Arc::new(NullSecretsProvider::default()),
        Arc::new(NullModel::new()),
        scope.clone(),
        keyspace.clone(),
    );
    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(StubAwaitApprovalSuite);
    let reg = Arc::new(reg);
    let mut state = ConnState::new(reg, suite_ctx.clone(), None);

    let thread_id = uuid::Uuid::new_v4().to_string();
    let store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
    let _ = store
        .append_step(
            &thread_id,
            ThreadStep::SwitchSuite {
                from: None,
                to: "suite_x".to_string(),
                observation: Observation::ok(),
                ts: chrono::Utc::now().to_rfc3339(),
                agent: "agent".to_string(),
            },
        )
        .await;

    let msg = json!({"v":1,"type":"open","cid":"c1","thread_id":thread_id,"suiteId":"suite_x","agentType":"agent","question":"Continue."});
    let mut sink = CollectSink::default();
    process_open(&msg, &mut state, &mut sink).await.unwrap();

    let reader = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
    let log = reader.get(&thread_id).await.unwrap();
    assert!(matches!(
        log.steps.last(),
        Some(ThreadStep::Interrupt { .. })
    ));
}

#[tokio::test]
async fn user_request_streams_thread_state_and_tool_events() {
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
    let suite_ctx = SuiteCtx::new(
        storage.clone(),
        Arc::new(NullSecretsProvider::default()),
        Arc::new(NullModel::new()),
        scope.clone(),
        keyspace.clone(),
    );

    // Seed a thread with durable suite/agent selection so process_user can derive context.
    let thread_id = uuid::Uuid::new_v4().to_string();
    let store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
    let _ = store
        .append_step(
            &thread_id,
            ThreadStep::SwitchSuite {
                from: None,
                to: "suite_x".to_string(),
                observation: Observation::ok(),
                ts: chrono::Utc::now().to_rfc3339(),
                agent: "agent".to_string(),
            },
        )
        .await;
    let _ = store
        .append_step(
            &thread_id,
            ThreadStep::SwitchAgent {
                from: None,
                to: "agent".to_string(),
                observation: Observation::ok(),
                ts: chrono::Utc::now().to_rfc3339(),
                agent: "agent".to_string(),
            },
        )
        .await;

    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(StubDataEngineerSuite);
    let reg = Arc::new(reg);
    let mut state = ConnState::new(reg, suite_ctx, None);

    let msg = json!({"v":1,"type":"user","cid":"c1","thread_id":thread_id,"text":"continue"});
    let mut sink = CollectSink::default();
    process_user(&msg, &mut state, &mut sink).await.unwrap();

    let frames = sink.out.lock().unwrap().clone();
    assert!(!frames.is_empty());
    let mut saw_thread_state = false;
    let mut saw_tool_start = false;
    let mut saw_tool_end = false;
    let mut saw_tool_phase = false;
    for s in frames {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                "thread_state" => saw_thread_state = true,
                "tool_start" => saw_tool_start = true,
                "tool_end" => saw_tool_end = true,
                _ => {}
            }
            if matches!(
                v.get("type").and_then(|t| t.as_str()),
                Some("tool_start" | "tool_end")
            ) {
                if v.get("phase")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .trim()
                    .is_empty()
                    == false
                {
                    saw_tool_phase = true;
                }
            }
        }
    }
    assert!(saw_thread_state);
    assert!(saw_tool_start);
    assert!(saw_tool_end);
    assert!(saw_tool_phase);
}
