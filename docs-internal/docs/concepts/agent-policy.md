# Agent Policy

The **agent policy** controls what the agent loop considers a valid complete result, when to interrupt execution for user input or approval, and what context to inject into the prompt. It is the primary extension point that keeps the core loop agnostic to domain logic.

## The AgentPolicy trait

```rust
#[async_trait]
pub trait AgentPolicy: Send + Sync {
    fn prelude_entries(&self, ctx: &AgentCtx, store: Option<&ThreadStore>, thread_id: &str)
        -> Vec<Entry>;

    fn interrupt_for_action(&self, action_name: &str, args: &Value, obs: &Value)
        -> Option<(InterruptKind, String)>;

    fn timeout_for_tool(&self, action_name: &str) -> Option<u64>;

    async fn handle_complete(&self, tools: &ToolRegistry, ctx: &AgentCtx,
        transcript: &mut Transcript, store: Option<&ThreadStore>,
        thread_id: &str, complete_env: &CompleteEnvelope)
        -> Result<CompleteDecision, String>;

    async fn fallback(&self, tools: &ToolRegistry, ctx: &AgentCtx,
        transcript: &mut Transcript, store: Option<&ThreadStore>,
        thread_id: &str)
        -> Result<RunOutcome, String>;
}
```

The transcript argument is [`Transcript`](../../../src/core/src/agent/transcript.rs), not a `Vec<String>` of role prefixes.

## Policy methods

### prelude_entries

Called before the loop starts. Returns typed [`Entry`](prelude) values (typically `Entry::Prelude`) injected into the prompt head between the system/tool-card and the user's question.

### interrupt_for_action

Called after each tool execution. If the policy returns `Some((InterruptKind, prompt))`, the loop pauses and sends an `await_user` or `await_approval` frame to the client. The client must respond before the loop continues.

This keeps the core loop tool-name-agnostic: the policy decides which tool actions warrant human intervention, not the loop itself.

### timeout_for_tool

Returns an optional per-tool timeout override in seconds. Tools that are expected to be slow (e.g. dbt validate) can have longer timeouts than the default `per_step_timeout_secs`.

### handle_complete

Called when the LLM produces a typed complete step. The policy validates the [`CompleteEnvelope`](../../../src/core/src/agent/mod.rs) and returns:

- `Ok(CompleteDecision::Accept { result })` — accept the complete and end the loop
- `Ok(CompleteDecision::Reject { reason })` — reject, push `Entry::CompletionRejected` onto the transcript, and continue the loop

This allows suites to enforce domain rules: for example, requiring that a complete includes valid SQL, or that all plan items have been completed.

### fallback

Called when the loop exhausts its step budget without an accepted complete. The default implementation returns `RunOutcome::Interrupt` with `InterruptKind::AwaitUser` asking the user to retry. Suites can override this to produce a best-effort result instead.

## Interrupt types

```rust
pub enum InterruptKind {
    AwaitUser,
    AwaitApproval,
}
```

- **AwaitUser** — the agent needs free-form input from the user (e.g. "Tell me about your business domain")
- **AwaitApproval** — the agent needs a binary approve/reject decision (e.g. "Approve the staging model for orders?")

## DefaultPolicy

A built-in policy that accepts any well-formed `CompleteEnvelope`, does not inject prelude lines, and does not interrupt on any tool action. Used by the KB suite and useful as a starting point for custom suites.

## Next steps

- [Extending: Custom Policy](../extending/custom-policy.md) — implementing your own policy
- [Flow Frames](flow-frames.md) — the output types produced by the loop
- [Suites](suites.md) — how suites wire policies into the agent context
