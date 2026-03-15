use std::sync::Arc;

use async_trait::async_trait;
use react_core::agent::{
    Agent, AgentCtxBuilder, AgentPolicy, CompleteDecision, CompleteEnvelope, DefaultPolicy,
    InterruptKind, RunOutcome,
};
use react_core::keyspace::DefaultKeyspace;
use react_core::llm::{ChatMessage, LargeLanguageModel};
use react_core::scope::RequestScope;
use react_core::session::ThreadStore;
use react_core::tools::{Tool, ToolRegistry};
use react_module_storage_memory::InMemoryStorageAdapter;
use serde_json::Value;

struct FixedJsonModel {
    out: String,
}

impl LargeLanguageModel for FixedJsonModel {
    fn chat(
        &self,
        _messages: &[ChatMessage],
        _options: &react_core::llm::LlmCallOptions,
    ) -> Result<String, String> {
        Ok(self.out.clone())
    }

    fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Ok(vec![vec![0.0; 8]])
    }
}

struct AskUserTool;

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &'static str {
        "ask_user"
    }
    async fn call(
        &self,
        _args: Value,
        _ctx: &react_core::agent::AgentCtx,
    ) -> Result<Value, String> {
        Ok(serde_json::json!({"ok": true, "prompt": "hi"}))
    }
}

struct InterruptOnAskUser;

#[async_trait]
impl AgentPolicy for InterruptOnAskUser {
    fn interrupt_for_action(
        &self,
        action_name: &str,
        _args: &Value,
        obs: &Value,
    ) -> Option<(InterruptKind, String)> {
        if action_name == "ask_user" {
            let prompt = obs
                .get("prompt")
                .and_then(|x| x.as_str())
                .unwrap_or("x")
                .to_string();
            return Some((InterruptKind::AwaitUser, prompt));
        }
        None
    }

    async fn handle_complete(
        &self,
        _tools: &ToolRegistry,
        _ctx: &react_core::agent::AgentCtx,
        _transcript: &mut Vec<String>,
        _store: Option<&ThreadStore>,
        _thread_id: &str,
        _complete_env: &CompleteEnvelope,
    ) -> Result<CompleteDecision, String> {
        Ok(CompleteDecision::Reject {
            reason: "interrupt_only_test_policy_rejects_complete".to_string(),
        })
    }
}

#[tokio::test]
async fn agent_default_policy_accepts_typed_complete() {
    let llm = Arc::new(FixedJsonModel {
        out: r#"{"type":"complete","name":null,"args":null,"complete":{"kind":"kb","payload":"{\"answer\":\"hello\"}","display":null}} "#.to_string(),
    });
    let ctx = AgentCtxBuilder::new(
        llm,
        Arc::new(InMemoryStorageAdapter::default()),
        RequestScope::parse("t", "w", "p").expect("valid test scope"),
        Arc::new(DefaultKeyspace::new("b".into())),
        Arc::new(DefaultPolicy),
    )
    .top_k(1)
    .per_step_timeout_secs(1)
    .max_steps(1)
    .agent_name("test".to_string())
    .build();
    let reg = ToolRegistry::new();
    let out = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        react_core::llm::LlmCallOptions {
            prompt_id: "react.tests.kb_suite_smoke.basic",
            thread_id: None,
            expected_format: react_core::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("run");
    match out {
        RunOutcome::Complete { result, .. } => {
            assert_eq!(result.kind.as_str(), "kb");
            assert_eq!(
                result.payload.get("answer").and_then(|x| x.as_str()),
                Some("hello")
            );
        }
        _ => panic!("expected complete"),
    }
}

#[tokio::test]
async fn agent_does_not_special_case_ask_user_tool_name() {
    let llm = Arc::new(FixedJsonModel {
        out: r#"{"type":"tool","name":"ask_user","args":"{}","complete":null} "#.to_string(),
    });
    let ctx = AgentCtxBuilder::new(
        llm,
        Arc::new(InMemoryStorageAdapter::default()),
        RequestScope::parse("t", "w", "p").expect("valid test scope"),
        Arc::new(DefaultKeyspace::new("b".into())),
        Arc::new(DefaultPolicy),
    )
    .top_k(1)
    .per_step_timeout_secs(1)
    .max_steps(1)
    .agent_name("test".to_string())
    .build();
    let mut reg = ToolRegistry::new();
    reg.register(AskUserTool);
    let out = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        react_core::llm::LlmCallOptions {
            prompt_id: "react.tests.kb_suite_smoke.ask_user_name_not_special_cased",
            thread_id: None,
            expected_format: react_core::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("run");
    match out {
        RunOutcome::Interrupt { kind, .. } => {
            assert_eq!(kind, react_core::agent::InterruptKind::AwaitUser);
        }
        other => panic!(
            "expected Interrupt fallback (no complete produced), got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[tokio::test]
async fn agent_interrupts_only_when_policy_requests_it() {
    let llm = Arc::new(FixedJsonModel {
        out: r#"{"type":"tool","name":"ask_user","args":"{}","complete":null} "#.to_string(),
    });
    let ctx = AgentCtxBuilder::new(
        llm,
        Arc::new(InMemoryStorageAdapter::default()),
        RequestScope::parse("t", "w", "p").expect("valid test scope"),
        Arc::new(DefaultKeyspace::new("b".into())),
        Arc::new(InterruptOnAskUser),
    )
    .top_k(1)
    .per_step_timeout_secs(1)
    .max_steps(1)
    .agent_name("test".to_string())
    .build();
    let mut reg = ToolRegistry::new();
    reg.register(AskUserTool);
    let out = Agent::run_until_block(
        &reg,
        &ctx,
        "sys",
        "tools",
        "q",
        react_core::llm::LlmCallOptions {
            prompt_id: "react.tests.kb_suite_smoke.policy_interrupts_only_when_requested",
            thread_id: None,
            expected_format: react_core::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
            model: None,
        },
    )
    .await
    .expect("run");
    match out {
        RunOutcome::Interrupt { kind, prompt, .. } => {
            assert_eq!(kind, react_core::agent::InterruptKind::AwaitUser);
            assert_eq!(prompt, "hi");
        }
        _ => panic!("expected Interrupt(AwaitUser)"),
    }
}

#[test]
fn default_registry_includes_kb_suite() {
    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(react_suite_data_engineer::DataEngineerSuite);
    reg.register(react_suite_kb::KbSuite);
    let ids = reg.list_ids();
    assert!(
        ids.contains(&"kb"),
        "expected 'kb' in suite registry, got {:?}",
        ids
    );
}
