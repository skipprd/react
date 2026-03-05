# Agent Policy

The **agent policy** controls what the agent loop considers a valid final result, when to interrupt execution for user input or approval, and what context to inject into the prompt. It is the primary extension point that keeps the core loop agnostic to domain logic.

## The AgentPolicy trait

```rust
#[async_trait]
pub trait AgentPolicy: Send + Sync {
    fn prelude_lines(&self, ctx: &AgentCtx, store: Option<&ThreadStore>, thread_id: &str)
        -> Vec<String>;

    fn interrupt_for_action(&self, action_name: &str, args: &Value, obs: &Value)
        -> Option<Interrupt>;

    fn timeout_for_tool(&self, action_name: &str) -> Option<u64>;

    async fn handle_final(&self, tools: &ToolRegistry, ctx: &AgentCtx,
        transcript: &mut Vec<String>, store: Option<&ThreadStore>,
        thread_id: &str, final_env: &FinalEnvelope)
        -> Result<Option<RunOutcome>, String>;

    async fn fallback(&self, tools: &ToolRegistry, ctx: &AgentCtx,
        transcript: &mut Vec<String>, store: Option<&ThreadStore>,
        thread_id: &str)
        -> Result<RunOutcome, String>;
}
```

## Policy methods

### prelude_lines

Called before each LLM call. Returns lines injected into the prompt between the system/tool-card header and the user's question. Suites use this to inject dynamic context — for example, the Data Engineer suite injects catalog metadata, plan state, and dataset summaries.

### interrupt_for_action

Called after each tool execution. If the policy returns `Some(Interrupt)`, the loop pauses and sends an `await_user` or `await_approval` frame to the client. The client must respond before the loop continues.

This keeps the core loop tool-name-agnostic: the policy decides which tool actions warrant human intervention, not the loop itself.

### timeout_for_tool

Returns an optional per-tool timeout override in seconds. Tools that are expected to be slow (e.g. dbt validate) can have longer timeouts than the default `per_step_timeout_secs`.

### handle_final

Called when the LLM produces a `{"type": "final", ...}` action. The policy validates the final envelope and returns:

- `Ok(Some(RunOutcome::Final{..}))` — accept the final and end the loop
- `Ok(None)` — reject the final, append feedback to the transcript, and continue the loop

This allows suites to enforce domain rules: for example, requiring that a final includes valid SQL, or that all plan items have been completed.

### fallback

Called when the loop exhausts its step budget without an accepted final. The default implementation returns `AwaitUser` with a message asking the user to retry. Suites can override this to produce a best-effort result instead.

## Interrupt types

```rust
pub enum Interrupt {
    AwaitUser { prompt: String },
    AwaitApproval { prompt: String },
}
```

- **AwaitUser** — the agent needs free-form input from the user (e.g. "Tell me about your business domain")
- **AwaitApproval** — the agent needs a binary approve/reject decision (e.g. "Approve the staging model for orders?")

## DefaultPolicy

A built-in policy that accepts any well-formed `FinalEnvelope`, does not inject prelude lines, and does not interrupt on any tool action. Used by the KB suite and useful as a starting point for custom suites.

## Next steps

- [Extending: Custom Policy](../extending/custom-policy.md) — implementing your own policy
- [Flow Frames](flow-frames.md) — the output types produced by the loop
- [Suites](suites.md) — how suites wire policies into the agent context
