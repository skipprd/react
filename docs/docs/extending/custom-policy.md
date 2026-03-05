# Custom Policy

Implement `AgentPolicy` to control final validation, interrupt behaviour, context injection, and per-tool timeouts.

## When to use a custom policy

- You need domain-specific final validation (e.g. "the final must include valid SQL")
- You want certain tool actions to pause for user approval
- You need to inject dynamic context into the prompt (e.g. plan state, catalog data)
- You want custom timeout behaviour for slow tools

## Implementation

```rust
use async_trait::async_trait;
use serde_json::Value;
use react_core::agent::{
    AgentCtx, AgentPolicy, FinalEnvelope, Interrupt, RunOutcome,
};
use react_core::session::ThreadStore;
use react_core::tools::ToolRegistry;

pub struct MyPolicy;

#[async_trait]
impl AgentPolicy for MyPolicy {
    fn prelude_lines(
        &self,
        ctx: &AgentCtx,
        store: Option<&ThreadStore>,
        thread_id: &str,
    ) -> Vec<String> {
        // Inject context before the user's question.
        // Example: inject plan state summary
        vec![
            "## Current plan status".to_string(),
            "- staging_orders: done".to_string(),
            "- gold_weekly_rides: in_progress".to_string(),
        ]
    }

    fn interrupt_for_action(
        &self,
        action_name: &str,
        _args: &Value,
        _obs: &Value,
    ) -> Option<Interrupt> {
        // Require approval before publishing
        if action_name == "publish_dbt_to_provider" {
            Some(Interrupt::AwaitApproval {
                prompt: "Approve publishing models to the warehouse?".to_string(),
            })
        } else {
            None
        }
    }

    fn timeout_for_tool(&self, action_name: &str) -> Option<u64> {
        match action_name {
            "dbt_validate" => Some(300),
            "sql_run" => Some(120),
            _ => None,
        }
    }

    async fn handle_final(
        &self,
        _tools: &ToolRegistry,
        ctx: &AgentCtx,
        transcript: &mut Vec<String>,
        store: Option<&ThreadStore>,
        thread_id: &str,
        final_env: &FinalEnvelope,
    ) -> Result<Option<RunOutcome>, String> {
        // Validate the final before accepting
        if final_env.kind == "ask" {
            let has_sql = final_env.payload.get("sql")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);

            if !has_sql {
                // Reject: append feedback and continue the loop
                transcript.push(
                    "Observation: Final rejected — ask results must include SQL.".to_string()
                );
                return Ok(None);
            }
        }

        // Accept the final
        let result = react_core::session::ThreadResult {
            kind: react_core::session::FinalKind::from(final_env.kind.clone()),
            payload: final_env.payload.clone(),
            display: final_env.display.clone(),
        };

        Ok(Some(RunOutcome::Final {
            thread_id: thread_id.to_string(),
            result,
        }))
    }

    async fn fallback(
        &self,
        _tools: &ToolRegistry,
        _ctx: &AgentCtx,
        _transcript: &mut Vec<String>,
        _store: Option<&ThreadStore>,
        thread_id: &str,
    ) -> Result<RunOutcome, String> {
        Ok(RunOutcome::AwaitUser {
            thread_id: thread_id.to_string(),
            prompt: "The agent couldn't complete the task. What would you like to try?".to_string(),
        })
    }
}
```

## Using the policy

Pass the policy when building the `AgentCtx` in your suite:

```rust
let ctx = AgentCtx {
    policy: Arc::new(MyPolicy),
    // ...other fields...
};
```

## Built-in policies

| Policy | Behaviour |
|---|---|
| `DefaultPolicy` | Accepts any well-formed final. No prelude, no interrupts, no custom timeouts. |

The Data Engineer suite uses `SqlValidatedPolicy` which validates that `ask` finals include SQL, injects plan and catalog context as prelude lines, and interrupts on approval-requiring tool actions.

## Next steps

- [Agent Policy concept](../concepts/agent-policy.md) — how policies fit into the agent loop
- [Custom Suite](custom-suite.md) — building suites that use custom policies
