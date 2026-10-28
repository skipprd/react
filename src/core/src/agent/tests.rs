use super::*;
use crate::error::CoreError;
use crate::keyspace::DefaultKeyspace;

use crate::scope::RequestScope;
use crate::test_support::InMemoryStorageAdapter;
use crate::tools::ToolRegistry;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct ScriptedModel {
    replies: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl crate::llm::LargeLanguageModel for ScriptedModel {
    async fn chat(
        &self,
        _messages: &[crate::llm::ChatMessage],
        _options: &crate::llm::LlmCallOptions,
    ) -> Result<String, crate::llm::LlmError> {
        let mut g = self
            .replies
            .lock()
            .map_err(|_| crate::llm::LlmError::protocol("mutex poisoned"))?;
        if g.is_empty() {
            return Err(crate::llm::LlmError::protocol("no more replies"));
        }
        Ok(g.remove(0))
    }

    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, crate::llm::LlmError> {
        Ok(vec![])
    }
}

struct CapturingOptionsModel {
    replies: Arc<Mutex<Vec<String>>>,
    last_opts: Arc<Mutex<Option<crate::llm::LlmCallOptions>>>,
}

#[async_trait]
impl crate::llm::LargeLanguageModel for CapturingOptionsModel {
    async fn chat(
        &self,
        _messages: &[crate::llm::ChatMessage],
        options: &crate::llm::LlmCallOptions,
    ) -> Result<String, crate::llm::LlmError> {
        if let Ok(mut g) = self.last_opts.lock() {
            *g = Some(options.clone());
        }
        let mut g = self
            .replies
            .lock()
            .map_err(|_| crate::llm::LlmError::protocol("mutex poisoned"))?;
        if g.is_empty() {
            return Err(crate::llm::LlmError::protocol("no more replies"));
        }
        Ok(g.remove(0))
    }

    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, crate::llm::LlmError> {
        Ok(vec![])
    }
}

struct TimeoutPolicy {
    inner: DefaultPolicy,
    secs: u64,
}

#[async_trait]
impl AgentPolicy for TimeoutPolicy {
    fn timeout_for_tool(&self, _action_name: &str) -> Option<u64> {
        Some(self.secs)
    }

    async fn handle_complete(
        &self,
        tools: &ToolRegistry,
        ctx: &AgentCtx,
        transcript: &mut Transcript,
        store: Option<&crate::session::ThreadStore>,
        thread_id: &str,
        complete_env: &CompleteEnvelope,
    ) -> Result<CompleteDecision, String> {
        self.inner
            .handle_complete(tools, ctx, transcript, store, thread_id, complete_env)
            .await
    }
}

struct SlowTool;

#[async_trait]
impl crate::tools::Tool for SlowTool {
    fn name(&self) -> &'static str {
        "slow_tool"
    }

    async fn call(&self, _args: Value, _ctx: &AgentCtx) -> Result<Value, String> {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        Ok(serde_json::json!({"ok": true}))
    }
}

#[tokio::test]
async fn per_tool_timeout_override_is_used() {
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec![
            "{\"type\":\"tool\",\"name\":\"slow_tool\",\"args\":\"{}\",\"complete\":null}".to_string(),
            "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{\\\"text\\\":\\\"ok\\\"}\",\"display\":null}}".to_string(),
        ])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");

    let mut reg = ToolRegistry::new();
    reg.register(SlowTool).expect("unique tool");

    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        budget: crate::budget::BudgetMeter::new(crate::budget::RunBudget::agent_steps(4)),
        cancel: crate::budget::Cancel::new(),
        thread_id: Some("tid".to_string()),
        progress_tx: None,
        pre_step_tx: None,
        trace_tx: None,
        agent_name: Some("test".to_string()),
        policy: Arc::new(TimeoutPolicy {
            inner: DefaultPolicy,
            secs: 3,
        }),
        llm,
        storage,
        scope,
        keyspace,
        capabilities: CapabilityMap::default(),
        vector: None,
        thread_store: None,
        exec_ctx: None,
        resolved_config: None,
        throttle_observer: None,
    };

    let out = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.run_until_block_basic",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("ok");
    match out {
        RunOutcome::Complete { result, .. } => {
            assert_eq!(result.kind.as_str(), "generic");
            assert_eq!(
                result.payload.get("text").and_then(|x| x.as_str()),
                Some("ok")
            );
        }
        _ => panic!("expected complete outcome"),
    }
}

#[tokio::test]
async fn run_until_block_passes_llm_call_options_through() {
    let last_opts: Arc<Mutex<Option<crate::llm::LlmCallOptions>>> = Arc::new(Mutex::new(None));
    let llm = Arc::new(CapturingOptionsModel {
        replies: Arc::new(Mutex::new(vec![
            "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{\\\"text\\\":\\\"ok\\\"}\",\"display\":null}}".to_string(),
        ])),
        last_opts: last_opts.clone(),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let reg = ToolRegistry::new();
    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        budget: crate::budget::BudgetMeter::new(crate::budget::RunBudget::agent_steps(2)),
        cancel: crate::budget::Cancel::new(),
        thread_id: Some("tid".to_string()),
        progress_tx: None,
        pre_step_tx: None,
        trace_tx: None,
        agent_name: Some("test".to_string()),
        policy: Arc::new(DefaultPolicy),
        llm,
        storage,
        scope,
        keyspace,
        capabilities: CapabilityMap::default(),
        vector: None,
        thread_store: None,
        exec_ctx: None,
        resolved_config: None,
        throttle_observer: None,
    };
    let opts = crate::llm::LlmCallOptions {
        prompt_id: "react_core.agent.tests.capture_options",
        thread_id: None,
        expected_format: crate::llm::LlmExpectedFormat::JsonObject,
        temperature: Some(0.9),
        top_p: Some(0.8),
        max_output_tokens: Some(1234),
        reasoning_effort: None,
        timeout_secs: None,
        model: None,
    };
    let _out = Agent::run_until_block(&reg, &ctx, "sys", "tools", "q", opts)
        .await
        .expect("ok");
    let got = last_opts.lock().ok().and_then(|g| g.clone()).expect("opts");
    assert_eq!(got.temperature, Some(0.9));
    assert_eq!(got.top_p, Some(0.8));
    assert_eq!(got.max_output_tokens, Some(1234));
}

#[tokio::test]
async fn complete_payload_round_trips_as_json_value() {
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec![
            "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{\\\"obj\\\":{\\\"hello\\\":\\\"world\\\",\\\"n\\\":1}}\",\"display\":null}}".to_string(),
        ])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");

    let reg = ToolRegistry::new();
    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        budget: crate::budget::BudgetMeter::new(crate::budget::RunBudget::agent_steps(2)),
        cancel: crate::budget::Cancel::new(),
        thread_id: Some("tid".to_string()),
        progress_tx: None,
        pre_step_tx: None,
        trace_tx: None,
        agent_name: Some("test".to_string()),
        policy: Arc::new(DefaultPolicy),
        llm,
        storage,
        scope,
        keyspace,
        capabilities: CapabilityMap::default(),
        vector: None,
        thread_store: None,
        exec_ctx: None,
        resolved_config: None,
        throttle_observer: None,
    };

    let out = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.complete_payload_round_trip",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("ok");
    match out {
        RunOutcome::Complete { result, .. } => {
            assert_eq!(result.kind.as_str(), "generic");
            let obj = result.payload.get("obj").expect("obj");
            assert_eq!(obj.get("hello").and_then(|x| x.as_str()), Some("world"));
            assert_eq!(obj.get("n").and_then(|x| x.as_i64()), Some(1));
        }
        _ => panic!("expected complete outcome"),
    }
}

#[test]
fn parse_agent_step_repairs_raw_newlines_inside_json_strings() {
    let raw =
        "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{\\\"text\\\":\\\"line1\nline2\\\"}\",\"display\":null}}";
    let step = Agent::parse_agent_step(raw).expect("should repair and parse");
    match step {
        AgentStep::Complete { complete_env } => {
            assert_eq!(complete_env.kind, "generic");
            assert_eq!(
                complete_env
                    .payload
                    .get("text")
                    .and_then(|x| x.as_str())
                    .unwrap(),
                "line1\nline2"
            );
        }
        _ => panic!("expected complete step"),
    }
}

#[test]
fn parse_agent_step_extracts_first_object_from_concatenated_json() {
    let raw = concat!(
        "{\"type\":\"tool\",\"name\":\"noop\",\"args\":\"{}\",\"complete\":null}",
        "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{}\",\"display\":null}}"
    );
    let step = Agent::parse_agent_step(raw).expect("should extract first JSON object");
    match step {
        AgentStep::Tool { name, .. } => assert_eq!(name, "noop"),
        _ => panic!("expected tool step from first JSON object"),
    }
}

#[test]
fn parse_agent_step_handles_invalid_escapes_in_args() {
    let raw = r#"{"type":"tool","name":"file","args":"{\"op\":\"patch\",\"path\":\"test.sql\",\"patch_text\":\"regexp_like(email, '^[^@\\s]+$')\"}","complete":null}"#;
    let step = Agent::parse_agent_step(raw).expect("should repair invalid escapes in args");
    match step {
        AgentStep::Tool { name, .. } => assert_eq!(name, "file"),
        _ => panic!("expected tool step"),
    }
}

struct NoopTool;
#[async_trait]
impl crate::tools::Tool for NoopTool {
    fn name(&self) -> &'static str {
        "noop"
    }
    async fn call(&self, _args: Value, _ctx: &AgentCtx) -> Result<Value, String> {
        Ok(serde_json::json!({"rows": [1, 2, 3]}))
    }
}

#[tokio::test]
async fn invalid_json_from_model_is_retried_with_minimal_prompt() {
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec![
            "{\"type\":\"tool\",\"name\":\"noop\",\"args\":\"{".to_string(),
            "{\"type\":\"tool\",\"name\":\"noop\",\"args\":\"{}\",\"complete\":null}".to_string(),
            "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{\\\"text\\\":\\\"ok\\\"}\",\"display\":null}}".to_string(),
        ])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");

    let mut reg = ToolRegistry::new();
    reg.register(NoopTool).expect("unique tool");

    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        budget: crate::budget::BudgetMeter::new(crate::budget::RunBudget::agent_steps(8)),
        cancel: crate::budget::Cancel::new(),
        thread_id: Some("tid".to_string()),
        progress_tx: None,
        pre_step_tx: None,
        trace_tx: None,
        agent_name: Some("test".to_string()),
        policy: Arc::new(DefaultPolicy),
        llm,
        storage,
        scope,
        keyspace,
        capabilities: CapabilityMap::default(),
        vector: None,
        thread_store: None,
        exec_ctx: None,
        resolved_config: None,
        throttle_observer: None,
    };

    let out = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.retry_loop",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("run should succeed after retry");

    match out {
        RunOutcome::Complete { thread_id, .. } => {
            assert_eq!(thread_id, "tid".to_string());
        }
        _ => panic!("expected complete outcome"),
    }
}

struct ScriptedResultModel {
    replies: Arc<Mutex<Vec<Result<String, crate::llm::LlmError>>>>,
}

#[async_trait]
impl crate::llm::LargeLanguageModel for ScriptedResultModel {
    async fn chat(
        &self,
        _messages: &[crate::llm::ChatMessage],
        _options: &crate::llm::LlmCallOptions,
    ) -> Result<String, crate::llm::LlmError> {
        let mut g = self
            .replies
            .lock()
            .map_err(|_| crate::llm::LlmError::protocol("mutex poisoned"))?;
        if g.is_empty() {
            return Err(crate::llm::LlmError::protocol("no more replies"));
        }
        g.remove(0)
    }

    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, crate::llm::LlmError> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn llm_fatal_error_is_not_retried() {
    let replies = Arc::new(Mutex::new(vec![
        Err(crate::llm::LlmError::Fatal(
            "You exceeded your current quota".into(),
        )),
        Ok("{\"type\":\"tool\",\"name\":\"noop\",\"args\":\"{}\",\"complete\":null}".to_string()),
    ]));
    let llm = Arc::new(ScriptedResultModel {
        replies: replies.clone(),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");

    let mut reg = ToolRegistry::new();
    reg.register(NoopTool).expect("unique tool");

    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        budget: crate::budget::BudgetMeter::new(crate::budget::RunBudget::agent_steps(8)),
        cancel: crate::budget::Cancel::new(),
        thread_id: Some("tid".to_string()),
        progress_tx: None,
        pre_step_tx: None,
        trace_tx: None,
        agent_name: Some("test".to_string()),
        policy: Arc::new(DefaultPolicy),
        llm,
        storage,
        scope,
        keyspace,
        capabilities: CapabilityMap::default(),
        vector: None,
        thread_store: None,
        exec_ctx: None,
        resolved_config: None,
        throttle_observer: None,
    };

    let res = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.llm_fatal_no_retry",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await;
    assert!(res.is_err(), "expected run to fail fast on fatal LLM error");
    let err = res.err().unwrap();

    assert!(
        matches!(err, CoreError::Llm(crate::llm::LlmError::Fatal(_))),
        "expected CoreError::Llm(Fatal); got: {err}"
    );
    assert_eq!(
        replies.lock().unwrap().len(),
        1,
        "agent should not have retried after fatal LLM error"
    );
}

#[tokio::test]
async fn throttle_observer_fires_on_first_transient() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let replies = Arc::new(Mutex::new(vec![
        Err(crate::llm::LlmError::Transient("429".into())),
        Ok("ok".to_string()),
    ]));
    let llm = Arc::new(ScriptedResultModel {
        replies: replies.clone(),
    });
    let fires = Arc::new(AtomicUsize::new(0));
    let fires_obs = fires.clone();
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        budget: crate::budget::BudgetMeter::new(crate::budget::RunBudget::agent_steps(8)),
        cancel: crate::budget::Cancel::new(),
        thread_id: Some("tid".to_string()),
        progress_tx: None,
        pre_step_tx: None,
        trace_tx: None,
        agent_name: Some("test".to_string()),
        policy: Arc::new(DefaultPolicy),
        llm,
        storage,
        scope,
        keyspace,
        capabilities: CapabilityMap::default(),
        vector: None,
        thread_store: None,
        exec_ctx: None,
        resolved_config: None,
        throttle_observer: Some(Arc::new(move |_| {
            fires_obs.fetch_add(1, Ordering::SeqCst);
        })),
    };

    let text = ctx
        .llm_chat(
            &[crate::llm::ChatMessage {
                role: crate::llm::ChatRole::User,
                content: "q".into(),
            }],
            &crate::llm::LlmCallOptions {
                prompt_id: "react_core.agent.tests.throttle_observer",
                thread_id: None,
                expected_format: crate::llm::LlmExpectedFormat::Text,
                max_output_tokens: None,
                temperature: None,
                top_p: None,
                reasoning_effort: None,
                timeout_secs: None,
                model: None,
            },
        )
        .await
        .expect("transient then success");
    assert_eq!(text, "ok");
    assert_eq!(fires.load(Ordering::SeqCst), 1);
    assert!(
        replies.lock().unwrap().is_empty(),
        "both scripted replies should have been consumed"
    );
}

struct CapturingFilesPatchTool {
    saw_patch: Arc<Mutex<bool>>,
}

#[async_trait]
impl crate::tools::Tool for CapturingFilesPatchTool {
    fn name(&self) -> &'static str {
        "file"
    }

    async fn call(&self, args: Value, _ctx: &AgentCtx) -> Result<Value, String> {
        let op = args.get("op").and_then(|v| v.as_str()).unwrap_or("");
        if op != "patch" {
            return Err(format!("expected op=patch, got op={op}"));
        }
        let has_rf = args.get("replace_file").is_some();
        let has_rr = args.get("replace_range").is_some();
        let has_rl = args.get("replace_list").is_some();
        let provided = (has_rf as usize) + (has_rr as usize) + (has_rl as usize);
        if provided != 1 {
            return Err("expected exactly one patch primitive".to_string());
        }
        if let Ok(mut g) = self.saw_patch.lock() {
            *g = true;
        }
        Ok(serde_json::json!({"ok": true}))
    }
}

#[tokio::test]
async fn patch_protocol_response_is_wrapped_as_file_patch_action() {
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec![
            serde_json::json!({
                "notes": ["example"],
                "replace_range": {
                    "path": "models/staging/stg_x.yml",
                    "start_line": 1,
                    "end_line": 10,
                    "new_text": "version: 2\n"
                }
            })
            .to_string(),
            serde_json::json!({
                "type": "tool",
                "name": "file",
                "args": "{\"op\":\"patch\",\"replace_range\":{\"path\":\"models/staging/stg_x.yml\",\"start_line\":1,\"end_line\":10,\"new_text\":\"version: 2\\n\"}}",
                "complete": null
            })
            .to_string(),
            "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{\\\"text\\\":\\\"ok\\\"}\",\"display\":null}}".to_string(),
        ])),
    });

    let saw_patch: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");

    let mut reg = ToolRegistry::new();
    reg.register(CapturingFilesPatchTool {
        saw_patch: saw_patch.clone(),
    })
    .expect("unique tool");

    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        budget: crate::budget::BudgetMeter::new(crate::budget::RunBudget::agent_steps(4)),
        cancel: crate::budget::Cancel::new(),
        thread_id: Some("tid".to_string()),
        progress_tx: None,
        pre_step_tx: None,
        trace_tx: None,
        agent_name: Some("test".to_string()),
        policy: Arc::new(DefaultPolicy),
        llm,
        storage,
        scope,
        keyspace,
        capabilities: CapabilityMap::default(),
        vector: None,
        thread_store: None,
        exec_ctx: Some({
            let mut ctx = ExecutionContext::default();
            ctx.set(
                "plan_kind",
                serde_json::Value::String("cleanse".to_string()),
            );
            ctx.set("plan_key", serde_json::Value::String("k".to_string()));
            ctx.set("workgroup_id", serde_json::Value::String("wg".to_string()));
            ctx.set("task_id", serde_json::Value::String("t".to_string()));
            ctx.set(
                "checklist_item_id",
                serde_json::Value::String("schema_contract".to_string()),
            );
            ctx.set("suite", serde_json::json!("suite_x"));
            ctx
        }),
        resolved_config: None,
        throttle_observer: None,
    };

    let out = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.file_patch_invoked",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("ok");

    let saw = match saw_patch.lock() {
        Ok(g) => *g,
        Err(_) => false,
    };
    assert!(saw, "expected file patch tool to be invoked");

    match out {
        RunOutcome::Complete { result, .. } => {
            assert_eq!(result.kind.as_str(), "generic");
        }
        _ => panic!("expected complete outcome"),
    }
}

struct InterruptOnNoopPolicy;

#[async_trait]
impl AgentPolicy for InterruptOnNoopPolicy {
    fn interrupt_for_action(
        &self,
        action_name: &str,
        _args: &Value,
        _obs: &Value,
    ) -> Option<(InterruptKind, String)> {
        if action_name == "noop" {
            return Some((InterruptKind::AwaitUser, "need input".to_string()));
        }
        None
    }

    async fn handle_complete(
        &self,
        tools: &ToolRegistry,
        ctx: &AgentCtx,
        transcript: &mut Transcript,
        store: Option<&ThreadStore>,
        thread_id: &str,
        complete_env: &CompleteEnvelope,
    ) -> Result<CompleteDecision, String> {
        DefaultPolicy
            .handle_complete(tools, ctx, transcript, store, thread_id, complete_env)
            .await
    }
}

#[tokio::test]
async fn run_until_block_non_interactive_suppresses_policy_interrupts() {
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec![
            "{\"type\":\"tool\",\"name\":\"noop\",\"args\":\"{}\",\"complete\":null}".to_string(),
            "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{\\\"text\\\":\\\"ok\\\"}\",\"display\":null}}".to_string(),
        ])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let mut reg = ToolRegistry::new();
    reg.register(NoopTool).expect("unique tool");
    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        budget: crate::budget::BudgetMeter::new(crate::budget::RunBudget::agent_steps(3)),
        cancel: crate::budget::Cancel::new(),
        thread_id: Some("tid".to_string()),
        progress_tx: None,
        pre_step_tx: None,
        trace_tx: None,
        agent_name: Some("test".to_string()),
        policy: Arc::new(InterruptOnNoopPolicy),
        llm,
        storage,
        scope,
        keyspace,
        capabilities: CapabilityMap::default(),
        vector: None,
        thread_store: None,
        exec_ctx: None,
        resolved_config: None,
        throttle_observer: None,
    };

    let out = Agent::run_until_block_non_interactive(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.non_interactive_interrupt_suppressed",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("non-interactive run should finish without AwaitUser");

    match out {
        RunOutcomeNonInteractive::Complete { result, .. } => {
            assert_eq!(result.kind.as_str(), "generic");
        }
        RunOutcomeNonInteractive::StepBoundary { .. } => {
            panic!("unexpected StepBoundary for interrupt-suppression test");
        }
    }
}

#[tokio::test]
async fn run_until_block_non_interactive_returns_step_boundary_on_step_budget_exhaustion() {
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec![
            "{\"type\":\"tool\",\"name\":\"noop\",\"args\":\"{}\",\"complete\":null}".to_string(),
        ])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let mut reg = ToolRegistry::new();
    reg.register(NoopTool).expect("unique tool");
    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        budget: crate::budget::BudgetMeter::new(crate::budget::RunBudget::agent_steps(1)),
        cancel: crate::budget::Cancel::new(),
        thread_id: Some("tid".to_string()),
        progress_tx: None,
        pre_step_tx: None,
        trace_tx: None,
        agent_name: Some("test".to_string()),
        policy: Arc::new(DefaultPolicy),
        llm,
        storage,
        scope,
        keyspace,
        capabilities: CapabilityMap::default(),
        vector: None,
        thread_store: None,
        exec_ctx: None,
        resolved_config: None,
        throttle_observer: None,
    };

    let out = Agent::run_until_block_non_interactive(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.non_interactive_budget_exhausted",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("non-interactive runner should return StepBoundary on step cap");
    match out {
        RunOutcomeNonInteractive::StepBoundary { thread_id, reason } => {
            assert_eq!(thread_id, "tid".to_string());
            assert_eq!(reason, StepBoundaryReason::StepBudgetExhausted);
        }
        other => panic!(
            "expected StepBoundary, got unexpected outcome: {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

struct RejectCompletePolicy;

#[async_trait]
impl AgentPolicy for RejectCompletePolicy {
    async fn handle_complete(
        &self,
        _tools: &ToolRegistry,
        _ctx: &AgentCtx,
        transcript: &mut Transcript,
        _store: Option<&ThreadStore>,
        _thread_id: &str,
        _complete_env: &CompleteEnvelope,
    ) -> Result<CompleteDecision, String> {
        let reason = "policy rejected complete in test".to_string();
        transcript.push(Entry::CompletionRejected {
            reason: reason.clone(),
        });
        Ok(CompleteDecision::Reject { reason })
    }
}

#[tokio::test]
async fn run_until_block_records_policy_blocked_stop() {
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec![
            "{\"type\":\"tool\",\"name\":\"noop\",\"args\":\"{}\",\"complete\":null}".to_string(),
        ])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn crate::keyspace::Keyspace> =
        Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let thread_store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
    let mut reg = ToolRegistry::new();
    reg.register(NoopTool).expect("unique tool");
    let ctx = AgentCtxBuilder::new(
        llm,
        storage,
        scope,
        keyspace,
        Arc::new(InterruptOnNoopPolicy),
    )
    .top_k(1)
    .per_step_timeout_secs(1)
    .max_steps(1)
    .thread_id("tid")
    .agent_name("test")
    .thread_store(thread_store.clone())
    .build();

    let out = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.policy_blocked_stop",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("run should interrupt");
    assert!(matches!(out, RunOutcome::Interrupt { .. }));

    let log = thread_store.get("tid").await.expect("log");
    assert!(log.steps.iter().any(|step| matches!(
        step,
        crate::session::ThreadStep::RunLoopStop {
            kind: crate::session::RunLoopStopKind::PolicyBlocked,
            ..
        }
    )));
}

#[tokio::test]
async fn run_until_block_records_rejected_complete_stop() {
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec![
            "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{\\\"text\\\":\\\"ok\\\"}\",\"display\":null}}".to_string(),
        ])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace: Arc<dyn crate::keyspace::Keyspace> =
        Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let thread_store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
    let reg = ToolRegistry::new();
    let ctx = AgentCtxBuilder::new(
        llm,
        storage,
        scope,
        keyspace,
        Arc::new(RejectCompletePolicy),
    )
    .top_k(1)
    .per_step_timeout_secs(1)
    .max_steps(1)
    .thread_id("tid")
    .agent_name("test")
    .thread_store(thread_store.clone())
    .build();

    let out = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.rejected_complete_stop",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("fallback interrupt should be returned");
    assert!(matches!(out, RunOutcome::Interrupt { .. }));

    let log = thread_store.get("tid").await.expect("log");
    assert!(log.steps.iter().any(|step| matches!(
        step,
        crate::session::ThreadStep::RunLoopStop {
            kind: crate::session::RunLoopStopKind::RejectedComplete,
            ..
        }
    )));
    assert!(log.steps.iter().any(|step| matches!(
        step,
        crate::session::ThreadStep::RunLoopStop {
            kind: crate::session::RunLoopStopKind::StepLimitExceeded,
            ..
        }
    )));
}

#[test]
fn parse_agent_step_does_not_panic_on_valid_json_wrong_shape() {
    let err = Agent::parse_agent_step("[1,2,3]").expect_err("wrong shape");
    assert!(matches!(
        err,
        crate::error::CoreError::Parse(_) | crate::error::CoreError::Schema(_)
    ));
}

#[tokio::test]
async fn llm_chat_json_does_not_panic_on_valid_json_wrong_shape() {
    #[derive(serde::Deserialize, Debug)]
    struct NeedType {
        #[serde(rename = "type")]
        _ty: String,
    }
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec![
            r#"{"foo":1}"#.to_string(),
            r#"{"foo":1}"#.to_string(),
        ])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        budget: crate::budget::BudgetMeter::new(crate::budget::RunBudget::agent_steps(2)),
        cancel: crate::budget::Cancel::new(),
        thread_id: Some("tid".to_string()),
        progress_tx: None,
        pre_step_tx: None,
        trace_tx: None,
        agent_name: Some("test".to_string()),
        policy: Arc::new(DefaultPolicy),
        llm,
        storage,
        scope,
        keyspace,
        capabilities: CapabilityMap::default(),
        vector: None,
        thread_store: None,
        exec_ctx: None,
        resolved_config: None,
        throttle_observer: None,
    };
    let err = ctx
        .llm_chat_json::<NeedType>(
            &[crate::llm::ChatMessage {
                role: crate::llm::ChatRole::User,
                content: "x".to_string(),
            }],
            &crate::llm::LlmCallOptions {
                prompt_id: "react_core.agent.tests.llm_chat_json_shape",
                thread_id: None,
                expected_format: crate::llm::LlmExpectedFormat::JsonObject,
                max_output_tokens: None,
                temperature: None,
                top_p: None,
                reasoning_effort: None,
                timeout_secs: None,
                model: None,
            },
        )
        .await
        .expect_err("wrong shape must be an error, not a panic");
    assert!(matches!(err, crate::error::CoreError::Parse(_)));
}

#[tokio::test]
async fn run_until_block_sends_system_and_user_roles() {
    let roles = Arc::new(Mutex::new(Vec::<crate::llm::ChatRole>::new()));
    struct RoleCaptureModel {
        replies: Arc<Mutex<Vec<String>>>,
        roles: Arc<Mutex<Vec<crate::llm::ChatRole>>>,
    }
    #[async_trait]
    impl crate::llm::LargeLanguageModel for RoleCaptureModel {
        async fn chat(
            &self,
            messages: &[crate::llm::ChatMessage],
            _options: &crate::llm::LlmCallOptions,
        ) -> Result<String, crate::llm::LlmError> {
            if let Ok(mut g) = self.roles.lock() {
                g.extend(messages.iter().map(|m| m.role.clone()));
            }
            let mut g = self
                .replies
                .lock()
                .map_err(|_| crate::llm::LlmError::protocol("mutex poisoned"))?;
            if g.is_empty() {
                return Err(crate::llm::LlmError::protocol("no more replies"));
            }
            Ok(g.remove(0))
        }
        async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, crate::llm::LlmError> {
            Ok(vec![])
        }
    }

    let llm = Arc::new(RoleCaptureModel {
        replies: Arc::new(Mutex::new(vec![
            "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{\\\"text\\\":\\\"ok\\\"}\",\"display\":null}}".to_string(),
        ])),
        roles: roles.clone(),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let ctx = AgentCtxBuilder::new(llm, storage, scope, keyspace, Arc::new(DefaultPolicy))
        .max_steps(2)
        .build();
    let _ = Agent::run_until_block(
        &ToolRegistry::new(),
        &ctx,
        "sys-prompt",
        "tools-card",
        "the-question",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.chat_roles",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("complete");
    let captured = roles.lock().expect("roles");
    assert_eq!(
        captured.first(),
        Some(&crate::llm::ChatRole::System),
        "first message must be System, got {captured:?}"
    );
    assert!(
        captured.contains(&crate::llm::ChatRole::System),
        "expected system role, got {captured:?}"
    );
    assert!(
        captured.contains(&crate::llm::ChatRole::User),
        "expected user role, got {captured:?}"
    );
}

#[test]
fn agent_ctx_prompt_budget_comes_from_resolved_llm() {
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec![])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let cfg = crate::resolved_config::ReactResolvedConfig {
        server: crate::resolved_config::ServerResolved { port: 1 },
        storage: crate::resolved_config::StorageResolved {
            mode: crate::resolved_config::StorageMode::Local,
            bucket: None,
            path: None,
            s3_credentials: None,
        },
        scope: scope.clone(),
        llm: crate::resolved_config::LlmResolved {
            context_length: Some(16_384),
            max_tokens: Some(2_048),
            ..Default::default()
        },
        suite_config: serde_json::json!({}),
    };
    let ctx = AgentCtxBuilder::new(llm, storage, scope, keyspace, Arc::new(DefaultPolicy))
        .resolved_config(Some(Arc::new(cfg)))
        .build();
    assert_eq!(ctx.prompt_budget(), PromptBudget::from_model(16_384, 2_048));
}

#[tokio::test]
async fn run_until_block_ok_value_without_ok_key_is_success() {
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec![
            "{\"type\":\"tool\",\"name\":\"noop\",\"args\":\"{}\",\"complete\":null}".to_string(),
            "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{\\\"text\\\":\\\"ok\\\"}\",\"display\":null}}".to_string(),
        ])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let mut reg = ToolRegistry::new();
    reg.register(NoopTool).expect("unique tool");
    let ctx = AgentCtxBuilder::new(llm, storage, scope, keyspace, Arc::new(DefaultPolicy))
        .max_steps(4)
        .thread_id("tid")
        .agent_name("test")
        .build();
    let out = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.ok_without_ok_key",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("Ok(Value) without ok key must succeed");
    match out {
        RunOutcome::Complete { thread_id, .. } => assert_eq!(thread_id, "tid"),
        _ => panic!("expected complete, got non-complete outcome"),
    }
}

#[tokio::test]
async fn cancelled_agent_ctx_returns_llm_cancelled() {
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec!["should not run".to_string()])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let cancel = crate::budget::Cancel::new();
    let ctx = AgentCtxBuilder::new(llm, storage, scope, keyspace, Arc::new(DefaultPolicy))
        .cancel(cancel.clone())
        .max_steps(2)
        .thread_id("tid")
        .agent_name("test")
        .build();
    cancel.cancel();
    let err = ctx
        .llm_chat(
            &[crate::llm::ChatMessage {
                role: crate::llm::ChatRole::User,
                content: "x".to_string(),
            }],
            &crate::llm::LlmCallOptions {
                prompt_id: "react_core.agent.tests.cancel",
                thread_id: None,
                expected_format: crate::llm::LlmExpectedFormat::Text,
                max_output_tokens: None,
                temperature: None,
                top_p: None,
                reasoning_effort: None,
                timeout_secs: None,
                model: None,
            },
        )
        .await
        .expect_err("cancelled");
    assert!(
        matches!(
            err,
            crate::error::CoreError::Llm(crate::llm::LlmError::Cancelled)
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn llm_call_budget_is_charged_per_attempt() {
    let meter = crate::budget::BudgetMeter::new(crate::budget::RunBudget {
        max_llm_calls: 1,
        max_tool_calls: 8,
        max_agent_steps: 8,
    });
    let llm = Arc::new(ScriptedModel {
        replies: Arc::new(Mutex::new(vec!["a".to_string(), "b".to_string()])),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
    let ctx = AgentCtxBuilder::new(llm, storage, scope, keyspace, Arc::new(DefaultPolicy))
        .budget(crate::budget::RunBudget {
            max_llm_calls: 1,
            max_tool_calls: 8,
            max_agent_steps: 8,
        })
        .thread_id("tid")
        .agent_name("test")
        .build();
    let opts = crate::llm::LlmCallOptions {
        prompt_id: "react_core.agent.tests.llm_budget",
        thread_id: None,
        expected_format: crate::llm::LlmExpectedFormat::Text,
        max_output_tokens: None,
        temperature: None,
        top_p: None,
        reasoning_effort: None,
        timeout_secs: None,
        model: None,
    };
    ctx.llm_chat(
        &[crate::llm::ChatMessage {
            role: crate::llm::ChatRole::User,
            content: "x".to_string(),
        }],
        &opts,
    )
    .await
    .expect("first call");
    let err = ctx
        .llm_chat(
            &[crate::llm::ChatMessage {
                role: crate::llm::ChatRole::User,
                content: "x".to_string(),
            }],
            &opts,
        )
        .await
        .expect_err("second call exceeds llm budget");
    assert!(
        matches!(
            err,
            crate::error::CoreError::Llm(crate::llm::LlmError::Budget(
                crate::budget::BudgetExhausted::LlmCalls
            )) | crate::error::CoreError::Budget(crate::budget::BudgetExhausted::LlmCalls)
        ),
        "{err:?}"
    );
    let _ = meter;
}
