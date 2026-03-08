use super::*;
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

impl crate::llm::LargeLanguageModel for ScriptedModel {
    fn chat(
        &self,
        _messages: &[crate::llm::ChatMessage],
        _options: &crate::llm::LlmCallOptions,
    ) -> Result<String, String> {
        let mut g = self
            .replies
            .lock()
            .map_err(|_| "mutex poisoned".to_string())?;
        if g.is_empty() {
            return Err("no more replies".to_string());
        }
        Ok(g.remove(0))
    }

    fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Ok(vec![])
    }
}

struct CapturingOptionsModel {
    replies: Arc<Mutex<Vec<String>>>,
    last_opts: Arc<Mutex<Option<crate::llm::LlmCallOptions>>>,
}

impl crate::llm::LargeLanguageModel for CapturingOptionsModel {
    fn chat(
        &self,
        _messages: &[crate::llm::ChatMessage],
        options: &crate::llm::LlmCallOptions,
    ) -> Result<String, String> {
        if let Ok(mut g) = self.last_opts.lock() {
            *g = Some(options.clone());
        }
        let mut g = self
            .replies
            .lock()
            .map_err(|_| "mutex poisoned".to_string())?;
        if g.is_empty() {
            return Err("no more replies".to_string());
        }
        Ok(g.remove(0))
    }

    fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
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
        transcript: &mut Vec<String>,
        store: Option<&crate::session::ThreadStore>,
        thread_id: &str,
        complete_env: &CompleteEnvelope,
    ) -> Result<Option<RunOutcome>, String> {
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
    reg.register(SlowTool);

    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        max_steps: 4,
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
        max_steps: 2,
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
        max_steps: 2,
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
        ParsedStep::Complete { complete_env } => {
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
fn parse_agent_step_rejects_concatenated_multiple_json_objects() {
    let raw = concat!(
        "{\"type\":\"tool\",\"name\":\"noop\",\"args\":\"{}\",\"complete\":null}",
        "{\"type\":\"complete\",\"name\":null,\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{}\",\"display\":null}}"
    );
    let err = Agent::parse_agent_step(raw).expect_err("should reject concatenation");
    assert!(
        err.to_string().contains("invalid JSON from model:"),
        "unexpected err: {err}"
    );
}

struct NoopTool;
#[async_trait]
impl crate::tools::Tool for NoopTool {
    fn name(&self) -> &'static str {
        "noop"
    }
    async fn call(&self, _args: Value, _ctx: &AgentCtx) -> Result<Value, String> {
        Ok(serde_json::json!({"ok": true}))
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
    reg.register(NoopTool);

    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        max_steps: 8,
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

#[tokio::test]
async fn llm_error_text_does_not_trigger_invalid_json_retries() {
    let replies = Arc::new(Mutex::new(vec![
        "LLM_ERROR: You exceeded your current quota".to_string(),
        "{\"type\":\"tool\",\"name\":\"noop\",\"args\":\"{}\",\"complete\":null}".to_string(),
    ]));
    let llm = Arc::new(ScriptedModel {
        replies: replies.clone(),
    });
    let storage = Arc::new(InMemoryStorageAdapter::default());
    let keyspace = Arc::new(DefaultKeyspace::new("bucket".to_string()));
    let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");

    let mut reg = ToolRegistry::new();
    reg.register(NoopTool);

    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        max_steps: 8,
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
    };

    let res = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        crate::llm::LlmCallOptions {
            prompt_id: "react_core.agent.tests.llm_error_no_retry",
            thread_id: None,
            expected_format: crate::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
        },
    )
    .await;
    assert!(res.is_err(), "expected run to fail fast on LLM_ERROR text");
    let err = res.err().unwrap();

    assert!(
        err.to_string().contains("LLM_ERROR:"),
        "expected error to include LLM_ERROR text; got: {err}"
    );
    assert_eq!(
        replies.lock().unwrap().len(),
        1,
        "agent should not have retried after LLM_ERROR text"
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
            return Err(format!("expected op=patch, got op={}", op));
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
    });

    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        max_steps: 4,
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
            ctx.set("plan_kind", serde_json::Value::String("cleanse".to_string()));
            ctx.set("plan_key", serde_json::Value::String("k".to_string()));
            ctx.set("workgroup_id", serde_json::Value::String("wg".to_string()));
            ctx.set("task_id", serde_json::Value::String("t".to_string()));
            ctx.set("checklist_item_id", serde_json::Value::String("schema_contract".to_string()));
            ctx.set("suite", serde_json::json!("suite_x"));
            ctx
        }),
        resolved_config: None,
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
        transcript: &mut Vec<String>,
        store: Option<&ThreadStore>,
        thread_id: &str,
        complete_env: &CompleteEnvelope,
    ) -> Result<Option<RunOutcome>, String> {
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
    reg.register(NoopTool);
    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        max_steps: 3,
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
    reg.register(NoopTool);
    let ctx = AgentCtx {
        top_k: 1,
        per_step_timeout_secs: 1,
        max_steps: 1,
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
        },
    )
    .await
    .expect("non-interactive runner should return StepBoundary on step cap");
    match out {
        RunOutcomeNonInteractive::StepBoundary { thread_id, reason } => {
            assert_eq!(thread_id, "tid".to_string());
            assert_eq!(reason, StepBoundaryReason::StepBudgetExhausted);
        }
        other => panic!("expected StepBoundary, got unexpected outcome: {:?}", std::mem::discriminant(&other)),
    }
}
