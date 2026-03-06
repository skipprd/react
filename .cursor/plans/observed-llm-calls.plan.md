---
name: ""
overview: ""
todos: []
isProject: false
---

# Single-Entry LLM & Dead Code Cleanup

## Problem

Two related tech debt items share a single root cause: suite-owned LLM calls bypass the core agent loop's observability, timeout, and retry infrastructure.

### Suite LLM calls are invisible to the runtime

The core agent loop (`run_loop.rs::llm_chat_once`) emits `ThreadStep::LlmStart` before and `ThreadStep::LlmEnd` after each LLM call. UIs and thread history can show in-flight LLM work, and phase timing projections are accurate.

~20 suite callsites call `ctx.llm.chat(...)` directly. They do NOT emit `LlmStart`/`LlmEnd`. Some emit a post-hoc `ThreadStep::LlmCall` (review_batched, patch_protocol, remediate), but planning calls (design memo, critique, enrichment, model candidates) emit nothing at all. Long-running LLM work looks idle in thread history and UI projections.

### Timeout and retry policy is fragmented


| Layer               | Timeout                                 | Retry                                             |
| ------------------- | --------------------------------------- | ------------------------------------------------- |
| `router.rs`         | `LLM_HTTP_TIMEOUT_SECS` (default 1200s) | HTTP status + transient, configurable max retries |
| `openai_compat.rs`  | `LLM_HTTP_TIMEOUT_SECS` (default 120s)  | Hardcoded 3 retries, 429 + transient              |
| `review_batched.rs` | none                                    | 1 JSON parse retry                                |
| `patch_protocol.rs` | none                                    | Up to 10 iteration loop with truncation retry     |
| `remediate.rs`      | none                                    | 1 retry on invalid spec                           |
| `mod.rs` (planning) | none                                    | 1 enrichment retry                                |


No callsite can override the per-call timeout. Suite-local retry logic is reimplemented at each site with different patterns for the same failure mode (malformed JSON).

## Design

### Core principle: one observed entry point

`ctx.llm_chat(...)` is the **sole entry point** for all LLM chat calls. Every LLM call automatically gets lifecycle events, observability recording, and per-call timeout support with zero discipline required from suite authors.

`AgentCtx.llm` remains `pub` because the struct is constructed via struct literals across crate boundaries (suites, runtime tests). Making it `pub(crate)` would require a constructor for a 16-field struct — new complexity rather than less. Enforcement is via grep verification: `\.llm\.chat\(` in suites returns zero hits.

Where there is no thread store (tests, lightweight contexts), the method gracefully skips event emission — the `Option<ThreadStore>` that `AgentCtx` already carries handles this naturally as a runtime no-op, not a separate code path.

### Phase 1: `AgentCtx::llm_chat` and `AgentCtx::llm_chat_json` ✅

Add two methods to `AgentCtx` (implemented in `src/core/src/agent/llm_gateway.rs`):

```rust
impl AgentCtx {
    /// Single entry point for all LLM chat calls. Emits LlmStart/LlmEnd/LlmCall
    /// thread events when a thread store and thread_id are available.
    pub async fn llm_chat(
        &self,
        messages: &[ChatMessage],
        options: &LlmCallOptions,
    ) -> Result<String, String>;

    /// Convenience: llm_chat + JSON parse with escape-repair and one retry.
    pub async fn llm_chat_json<T: DeserializeOwned>(
        &self,
        messages: &[ChatMessage],
        options: &LlmCallOptions,
    ) -> Result<T, String>;
}
```

`llm_chat` behavior:

1. If `self.thread_store` and `self.thread_id` are `Some`: append `ThreadStep::LlmStart`.
2. If `options.timeout_secs` is `Some(t)`: wrap in `tokio::time::timeout`.
3. Call the underlying LLM via `spawn_blocking` (the `LargeLanguageModel::chat` trait is sync/blocking; this is what `llm_chat_once` already does).
4. If thread store available: append `ThreadStep::LlmEnd`.
5. If `llm_calls_enabled()` and thread store available: append `ThreadStep::LlmCall` with prompt hash, parts, response hash.
6. Return `Ok(text)` or `Err`.

`llm_chat_json` behavior:

1. Call `self.llm_chat(messages, options)`.
2. Attempt `serde_json::from_str::<T>(&text)`.
3. On parse failure: apply `escape_control_chars_in_json_strings` and retry parse.
4. On second failure: append a follow-up user message ("Return ONLY valid JSON..."), call `self.llm_chat(...)` once more, parse again.
5. Return `Ok(T)` or `Err` with parse error context.

Note: the signature does NOT take `&ThreadStore` or `thread_id` — these come from `self`. Callsites become `ctx.llm_chat(&messages, &opts).await` with no extra ceremony. This is simpler than the original plan's signature.

### Phase 2: Add `timeout_secs` to `LlmCallOptions` ✅

```rust
pub struct LlmCallOptions {
    // ... existing fields ...
    pub timeout_secs: Option<u64>,
}
```

`llm_chat` reads this and applies `tokio::time::timeout`. The transport layer (`router.rs`) continues to enforce its own HTTP timeout as a safety net. The per-call timeout bounds the full call including transport retries.

### Phase 3: Refactor `run_loop.rs::llm_chat_once` to delegate ✅

`llm_chat_once` currently contains ~200 lines of LlmStart/LlmEnd/LlmCall emission, `spawn_blocking`, and observability hash computation. After Phase 1, this reduces to:

```rust
let text = ctx.llm_chat(&messages, &llm_options).await?;
```

The ReAct loop in `run_until_block` stays focused on prompt assembly, parsing, and step dispatch.

### Phase 4: Migrate suite callsites ✅

Replace all `ctx.llm.chat(...)` / `sctx.llm.chat(...)` calls with `ctx.llm_chat(...)` or `ctx.llm_chat_json(...)`. Since `llm` is now private, any missed site is a compile error.

**Callsites and what changes at each:**


| File                | Function(s)                                                                                                                                                 | Current                                                                     | After                                                                                                       |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `mod.rs`            | `generate_design_memo`, `critique_design_memo`, `revise_design_memo`, `generate_model_candidates`, `enrich_cleanse_tasks`, `enrich_model_tasks` (~10 calls) | `ctx.llm.chat(...)`, no events                                              | `ctx.llm_chat(...)`                                                                                         |
| `review_batched.rs` | `llm_json` (2 branches, ~130 lines of manual observability + JSON retry)                                                                                    | `ctx.llm.chat(...)`, manual LlmCall, manual JSON retry, two forked branches | `ctx.llm_chat_json(...)` — entire function collapses to ~20 lines                                           |
| `patch_protocol.rs` | `llm_patch_loop_single_file`                                                                                                                                | `ctx.llm.chat(...)`, manual LlmCall                                         | `ctx.llm_chat(...)` (keeps its own patch-specific retry loop — that's domain logic, not generic JSON retry) |
| `remediate.rs`      | `llm_should_remediate_sql`, `remediate_dbt_sql_keys_with_llm`, `repair_grounded_dbt_errors`, `remediate_unresolved_columns` (4 calls)                       | `ctx.llm.chat(...)`, manual observability helper                            | `ctx.llm_chat(...)` or `ctx.llm_chat_json(...)`                                                             |
| `catalog_note.rs`   | `CatalogNoteTool::call`                                                                                                                                     | `llm.chat(...)` with manual LlmCall                                         | `ctx.llm_chat(...)`                                                                                         |
| `sql_first.rs`      | `llm_draft_sql_json`                                                                                                                                        | `ctx.llm.chat(...)`                                                         | `ctx.llm_chat_json(...)`                                                                                    |
| `dbt_error.rs`      | `summarize_dbt_failure_llm`                                                                                                                                 | Takes `&dyn LargeLanguageModel`, bare `.chat()`                             | Change signature to take `&AgentCtx`, use `ctx.llm_chat_json(...)`                                          |


### Phase 5: Delete dead code ✅

Code that becomes dead after migration:


| What                                                                                        | Location                                               | ~Lines                  |
| ------------------------------------------------------------------------------------------- | ------------------------------------------------------ | ----------------------- |
| `record_llm_call_observability_async` + `record_llm_call_observability`                     | `remediate.rs:16–133`                                  | ~120                    |
| Two-branch `llm_calls_enabled()` fork + manual `ThreadStep::LlmCall` + manual JSON retry    | `review_batched.rs:542–679`                            | ~130 (collapses to ~20) |
| Manual `ThreadStep::LlmCall` construction + `llm_calls_enabled()` checks + hash computation | `patch_protocol.rs:428–603`, `catalog_note.rs:139–210` | ~80                     |
| `escape_control_chars_in_json_strings` duplicate                                            | `review_batched.rs:93–...`                             | ~30                     |
| `LlmSession` + `chat_strict` (dead today — nobody uses it)                                  | `session.rs:42–75`                                     | ~35                     |
| `LlmCallOptions::new` (sole callsites in `review_batched.rs` disappear)                     | `llm.rs:53–65`                                         | ~12                     |
| `summarize_dbt_failure_llm` raw-LLM signature (replaced by `&AgentCtx` signature)           | `dbt_error.rs`                                         | refactor, not deletion  |


**Estimated net: ~400 lines deleted, ~60 lines added for the two `AgentCtx` methods.**

### Phase 6: Unify transport timeout defaults ✅

`openai_compat.rs` defaults `LLM_HTTP_TIMEOUT_SECS` to 120s; `router.rs` defaults to 1200s. Production goes through `RouterModel` → `router.rs`. `openai_compat.rs` is only used directly in tests.

- Align `openai_compat.rs` to read the same resolved config path as `router.rs`, or remove its independent timeout parsing entirely.
- Same for `LLM_MAX_TOKENS` inconsistency (1024 in `session.rs` vs 8192 in `openai_compat.rs`).

## Out of scope

- `**ctx.llm.embed(...)`**: Embed calls are lightweight, infrequent, and don't need LlmStart/LlmEnd visibility. They can stay as direct field access via a dedicated `ctx.llm_embed(...)` method for consistency, but this is low priority and independent.
- **Catalog provider LLM calls** (`runtime/src/providers/catalog/enrich.rs`): These use their own `Arc<dyn LargeLanguageModel>` from the catalog provider, not `AgentCtx`. Different subsystem, different lifecycle.
- `**synthesize_title`** (`ws/server.rs`): WS-layer call using `suite_ctx.llm`. Not an agent call.
- `**patch_protocol.rs` domain retry loop** (truncation retry, no-op detection, idempotence guard): This is patch-specific domain logic, not generic JSON retry. It stays as-is but calls `ctx.llm_chat(...)` instead of `ctx.llm.chat(...)`.

## Constraints

- Hard cutover. No feature flags, no dual-path, no migration.
- No new traits or abstractions. Two methods on an existing struct using existing types.
- Grep-enforced: `\.llm\.chat\(` in suites returns zero hits. `llm` stays `pub` for struct-literal construction.
- `AgentCtx` already carries `thread_store: Option<ThreadStore>` and `thread_id: Option<String>` — the wrapper reads these, no extra arguments needed.

## Verification

- `cargo check` clean, zero warnings.
- All existing tests pass.
- Grep for `\.llm\.chat(` in suites returns zero hits (only `llm_chat` / `llm_chat_json` method calls remain).
- Grep for manual `ThreadStep::LlmStart` / `ThreadStep::LlmEnd` / `ThreadStep::LlmCall` construction in suites returns zero hits.
- Grep for `escape_control_chars_in_json_strings` returns one definition (in core or a shared util) and zero inline usages at suite callsites.
- Grep for `record_llm_call_observability` returns zero hits.
- Grep for `LlmSession` / `chat_strict` returns zero hits.

## Files involved

**Core (new/modified):**

- `src/core/src/agent/mod.rs` — register `llm_gateway` module
- `src/core/src/agent/llm_gateway.rs` — `llm_chat`, `llm_chat_json`, `llm_embed`, shared `escape_control_chars_in_json_strings`
- `src/core/src/llm.rs` — add `timeout_secs` to `LlmCallOptions`, remove `LlmCallOptions::new`
- `src/core/src/agent/run_loop.rs` — refactor `llm_chat_once` to delegate to `ctx.llm_chat`

**Suites (migrate + delete dead code):**

- `src/suites/react-suites/src/data_engineer/mod.rs` — planning calls
- `src/suites/react-suites/src/data_engineer/review_batched.rs` — collapse `llm_json`, delete observability fork + duplicate `escape_control_chars_in_json_strings`
- `src/suites/react-suites/src/data_engineer/patch_protocol.rs` — swap to `ctx.llm_chat`, delete manual LlmCall emission
- `src/suites/react-suites/src/data_engineer/dbt_repair/remediate.rs` — swap to `ctx.llm_chat`, delete `record_llm_call_observability`*
- `src/suites/react-suites/src/data_engineer/tools/catalog_note.rs` — swap to `ctx.llm_chat`, delete manual LlmCall emission
- `src/suites/react-suites/src/data_engineer/tools/sql_first.rs` — swap to `ctx.llm_chat_json`
- `src/suites/react-suites/src/data_engineer/dbt_error.rs` — change `summarize_dbt_failure_llm` to take `&AgentCtx`
- `src/suites/react-suites/src/data_engineer/control_flow.rs` — update caller of `summarize_dbt_failure_llm`
- `src/suites/react-suites/src/data_engineer/tools/dbt_validate.rs` — update caller of `summarize_dbt_failure_llm`

**Runtime (cleanup):**

- `src/runtime/src/llm/session.rs` — delete dead `LlmSession` + `chat_strict`
- `src/runtime/src/llm/openai_compat.rs` — align timeout default with router

