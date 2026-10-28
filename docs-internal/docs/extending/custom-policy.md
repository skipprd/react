# Custom Policy

Implement `AgentPolicy` to control complete validation, interrupt behaviour, context injection, and per-tool timeouts.

## When to use a custom policy

- You need domain-specific complete validation (e.g. "the complete must include valid SQL")
- You want certain tool actions to pause for user approval
- You need to inject dynamic context into the prompt (e.g. plan state, catalog data)
- You want custom timeout behaviour for slow tools

## Implementation

```rust
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use react_core::agent::{
    AgentCtxBuilder, AgentPolicy, CompleteDecision, CompleteEnvelope, Entry,
    InterruptKind, RunOutcome, Transcript,
};
use react_core::session::{ThreadResult, ThreadStore};
use react_core::tools::ToolRegistry;

pub struct MyPolicy;

#[async_trait]
impl AgentPolicy for MyPolicy {
    fn prelude_entries(
        &self,
        _ctx: &react_core::agent::AgentCtx,
        _store: Option<&ThreadStore>,
        _thread_id: &str,
    ) -> Vec<Entry> {
        vec![Entry::Prelude("## Current plan status\n- staging_orders: done".into())]
    }

    fn interrupt_for_action(
        &self,
        action_name: &str,
        _args: &Value,
        _obs: &Value,
    ) -> Option<(InterruptKind, String)> {
        if action_name == "publish_dbt_to_provider" {
            Some((
                InterruptKind::AwaitApproval,
                "Approve publishing models to the warehouse?".to_string(),
            ))
        } else {
            None
        }
    }

    fn timeout_for_tool(&self, action_name: &str) -> Option<u64> {
        match action_name {
            "dbt_validate" => Some(300),
            "run_sql" => Some(120),
            _ => None,
        }
    }

    async fn handle_complete(
        &self,
        _tools: &ToolRegistry,
        _ctx: &react_core::agent::AgentCtx,
        transcript: &mut Transcript,
        _store: Option<&ThreadStore>,
        _thread_id: &str,
        complete_env: &CompleteEnvelope,
    ) -> Result<CompleteDecision, String> {
        if complete_env.kind == "ask" {
            let has_sql = complete_env
                .payload
                .get("sql")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);

            if !has_sql {
                let reason = "Ask completions must include SQL.".to_string();
                transcript.push(Entry::CompletionRejected {
                    reason: reason.clone(),
                });
                return Ok(CompleteDecision::Reject { reason });
            }
        }

        let result = ThreadResult {
            kind: complete_env.kind.clone(),
            payload: complete_env.payload.clone(),
            display: complete_env.display.clone(),
        };
        Ok(CompleteDecision::Accept { result })
    }

    async fn fallback(
        &self,
        _tools: &ToolRegistry,
        _ctx: &react_core::agent::AgentCtx,
        _transcript: &mut Transcript,
        _store: Option<&ThreadStore>,
        thread_id: &str,
    ) -> Result<RunOutcome, String> {
        Ok(RunOutcome::Interrupt {
            thread_id: thread_id.to_string(),
            kind: InterruptKind::AwaitUser,
            prompt: "The agent couldn't complete the task. What would you like to try?".to_string(),
        })
    }
}
```

## Using the policy

Pass the policy when building the `AgentCtx` in your suite:

```rust
let ctx = AgentCtxBuilder::new(llm, storage, scope, keyspace, Arc::new(MyPolicy)).build();
```

## Built-in policies

| Policy | Behaviour |
|---|---|
| `DefaultPolicy` | Accepts any well-formed complete. No prelude, no interrupts, no custom timeouts. |

The Data Engineer suite uses `SqlValidatedPolicy` which validates that `ask` completions include SQL, injects plan and catalog context as prelude entries, and interrupts on approval-requiring tool actions.

## Next steps

- [Agent Policy concept](../concepts/agent-policy.md) — how policies fit into the agent loop
- [Custom Suite](custom-suite.md) — building suites that use custom policies
