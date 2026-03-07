# Tech Debt

All previously identified items have been resolved. No open tech debt remains.

---

## Resolved

### Unbounded Observability Caches (resolved)
`PART_SEEN_CACHE` and `CALL_ID_CACHE` now have `MAX_PART_SEEN_ENTRIES` / `MAX_CALL_ID_ENTRIES` limits (10,000). Caches are cleared when they exceed the limit.

### `run_until_block` Split Into Helpers (resolved)
Extracted `execute_tool_call` and `append_observation_to_transcript` from the 257-line function.

### `llm_chat` Split Into Helpers (resolved)
Extracted `emit_llm_start_step`, `emit_llm_end_step`, `record_llm_observation` from the 178-line function.

### `ChatMessage.role` Is a `ChatRole` Enum (resolved)
Replaced `String` role with `ChatRole` enum (`System`, `User`, `Assistant`) with serde support and `Display` impl. Updated all call sites across core, runtime, and suites.

### `Result<T, String>` Replaced with `CoreError` (resolved)
Added `thiserror`-based `CoreError` enum with variants (`Generic`, `Storage`, `Serialization`, `Session`, `Schema`, `Agent`, `Keyspace`). `From<String>` impl provides backward compatibility. `CoreResult<T>` type alias added.

### Error Context Added to `map_err` Calls (resolved)
All `map_err(|e| e.to_string())` calls in `storage.rs` and `store_io.rs` now include operation name and key context.

### `schema_registry` Returns `Result` (resolved)
`schema_for_id` and `strict_json_schema_for` return `Result` instead of panicking with `.expect()`.

### Shared `CapabilityMap` Extracted (resolved)
`CapabilityMap` type in `capability.rs` replaces duplicated `HashMap<TypeId, Arc<dyn Any>>` + methods in `AgentCtx` and `SuiteCtx`.

### `FlowFrame` Kind Is `FlowKind` Newtype (resolved)
`FlowFrame` variants use `FlowKind(String)` newtype wrapper instead of raw `String` for the `kind` field.

### Magic Numbers Extracted to Named Constants (resolved)
Core: `MIN_EXCERPT_CHARS`, `DEFAULT_MAX_CHARS`, `MAX_TOTAL_CHARS`, `KEYWORD_CONTEXT_LINES_AFTER`, `CACHE_TTL_SECS`, `RESERVOIR_CAPACITY`, `HISTOGRAM_BIN_COUNT`, `MAX_EXAMPLE_VALUES`.
Runtime: `DEFAULT_SERVER_PORT`, `DEFAULT_LOCAL_STORAGE_PATH`, `LOCK_CONTENTION_WARN_MS`, `DEFAULT_EVENT_HUB_CAPACITY`, `SENT_BUFFER_CAPACITY`.

### Test Env Var Mutation Cleaned Up (resolved)
`discover/stats.rs` test saves and restores `STATS_HISTOGRAM_ENABLED`. Config tests use `with_clean_env` wrapper with save/restore.

### `AgentCtx`, `SuiteCtx`, `ThreadStore` Have Manual `Debug` (resolved)
Manual `Debug` implementations added (fixed-format output suitable for logging).

### `std::process::exit()` Replaced with `Result` (resolved)
`build_suite_ctx` returns `Result<SuiteCtx, String>`. `wire_providers` returns `Result<(), String>`. `main.rs` handles errors at the binary entrypoint.

### `llama_cpp` Stale References Fixed (resolved)
Removed dead `crate::helpers::s3` calls, added local stubs. Fixed missing `options` parameter in `inner::chat`.

### `eprintln!` Replaced with Tracing (resolved)
All `eprintln!("ERROR: ...")` in `main.rs`, `bootstrap.rs`, `suite_wiring/data_engineer.rs` replaced with `tracing::error!`.

### `handle_message` Split Into Per-Type Handlers (resolved)
Extracted `handle_list_message`, `handle_suites_message`, `handle_new_message`, `handle_open_message`, `handle_user_message`, `handle_history_message`, `handle_seen_message`, `handle_plans_message`, `handle_thread_state_message`, `handle_delete_message`. `handle_message` is now a thin dispatcher.

### LLM Helper Code Deduplicated (resolved)
`pretty_json` moved to `llm/types.rs`. `OaiChatMessage`, `OaiChatReq`, and related structs consolidated in `llm/types.rs`. `text_from_part` / `extract_text` deduplicated into `extract_response_text`.

### `unwrap()` on JSON Serialization Replaced (resolved)
All `serde_json::to_string(...).unwrap()` in `ws/handlers.rs` and `ws/suite_runner.rs` replaced with `?` propagation.

### Process-Global Singletons: `RuntimeContext` Introduced (resolved)
`RuntimeContext` struct wrapping `Arc<ReactResolvedConfig>` created as the injected-context path. `build_runtime_context()` bridges from the global config.

### `Final` → `Complete` Naming Bridged (resolved)
Type aliases (`CompleteResult`, `CompleteResponse`, `AskCompletePayload`, etc.) and doc comments document the WS API "Final" ↔ core "Complete" mapping.

### Config Resolution Made Suite-Agnostic (resolved)
`resolve_suite_providers()` isolates the suite-specific coupling to a single documented function, the future extension point for multi-suite registry.

### Phase-Stage Name Mapping Made Data-Driven (resolved)
`PHASE_STAGE_WORKGROUP_MAP` constant and `workgroup_kinds_for_stage()` lookup replace hardcoded match arms.

### `"ask"` Default Agent Type Centralized (resolved)
`DEFAULT_AGENT_TYPE` constant in `ws/util.rs` replaces all hardcoded `"ask"` fallbacks.

### `println!` in `llama_cpp.rs` Replaced (resolved)
All `println!` calls replaced with `tracing::info!` / `tracing::debug!`.

### Config Test Env Lock Fixed (resolved)
`with_clean_env` wrapper saves/restores env vars (handles panics via `catch_unwind`).

### `complete` Envelope Overload Fixed with `Checkpoint` Variant (resolved)
Added `FlowFrame::Checkpoint` variant for non-terminal handoffs. `plan_discovery_ready` now uses `Checkpoint` instead of `Complete`.

### Post-Discovery Planning Made Visible (resolved)
Progress messages added at key boundaries in `phase_plan.rs`: plan compilation, grounding, and validation stages.

### `Continue` Replaced with Typed Executor Outcomes (resolved)
`PhaseExecutorOutcome::Continue` split into `StayInPhase` and `TransitionCommitted` with clear semantic meaning.

### Typed Plan Wrappers at Control-Flow Boundaries (resolved)
`PlanStatus` has validated `try_approve()`, `try_complete()`, `try_cancel()` transition methods. `TrackPlan` convenience methods used at approval boundaries.

### Cleanse/Model Pipelines Unified (resolved)
`finalize_plan_and_approve()` shared helper extracts duplicated tail logic from both branches.

### `NonInteractivePolicyAdapter` Delegates `clean_tool_name` (resolved)
One-line fix: delegates to inner policy instead of falling back to `title_case_words`.

### Process-Global Thread Log Cache Removed (resolved)
`THREAD_CACHE` static removed. Cache is a per-instance `Arc<DashMap>` field on `ThreadStore`.

### Domain-Specific Strings Scrubbed from Core (resolved)
All `"dbt"`, `"catalog"`, `"data_engineer"`, `"warehouse"`, `"LanceDB"` references removed from core comments, docstrings, and test data.

### Config YAML Shapes Moved to Suite (resolved)
`ProvidersFile`, `WarehouseFile`, `CatalogFile`, `DbtFile`, `DbtNamingFile`, `VectorFile` moved from `config.rs` to `de_config.rs` with `resolve_providers_from_yaml()`. `config.rs` passes `providers: Option<serde_json::Value>`.

### `getenv_nonempty` Deduplicated (resolved)
Canonical `getenv_nonempty`, `getenv_usize`, `getenv_u64` in `runtime_settings.rs`.

### Bootstrap Decoupled from data_engineer (resolved)
`bootstrap.rs` is a generic dispatcher. Suite-specific wiring in `suite_wiring/data_engineer.rs`.

### WS Result Mapping Is Payload-Driven (resolved)
Inspects payload structure instead of matching on kind strings.

### `"preflight"` Centralised as Constant (resolved)
All literals replaced with `DEFAULT_INITIAL_PHASE` constant.

### Tool Extra-Key Forwarding Is Data-Driven (resolved)
Merges all `observation.extra` keys unconditionally.

### Domain Strings Replaced in WS Test Code (resolved)
`"cleanse"` replaced with generic test data in server.rs and terminal.rs test fixtures.

### DBT Terminal Logic Extracted (resolved)
`DbtValidateKind`, helpers, and `DBT_VALIDATE_TOOL_NAME` extracted to `ws/terminal_dbt.rs`.

### server.rs Decomposed (resolved)
Handlers extracted to `ws/handlers.rs`. Dispatch to `ws/protocol.rs`. `server.rs` reduced to ~1343 lines.

### Dead Code in LLM Adapters Removed (resolved)
Unused `RespResp` structs removed from `openai_compat.rs` and `openai_responses_adapter.rs`.

### Suite-Specific Logic Extracted from Runtime (resolved)
Provider modules moved to suite-owned crates. Suite-specific re-exports removed.

### `main.rs` Provider Wiring Deduplicated (resolved)
`build_suite_ctx()` extracted to `bootstrap.rs`.

### `config.rs` Decoupled from Suite Imports (resolved)
Passes `suite_config: serde_json::Value` to suites.

### Dual Config Access Paths Unified (resolved)
`helpers/configuration.rs` deleted. `runtime_settings.rs` is the single config source.

### ws/server.rs Initial Decomposition (resolved)
Extracted `util.rs`, `mapping.rs`, `conn_state.rs`, `thread_state.rs`, `suite_runner.rs`, `history.rs`.

### Re-Export Shim Modules Removed (resolved)
Deleted `adapters/`, `discover/`, unused provider modules, `util/dedup.rs`.

### `lazy_static!` Replaced (resolved)
Uses `std::sync::LazyLock`. `lazy_static` dependency removed.

### `CHAT_MEMO` Capped (resolved)
256-entry cap with TTL-based eviction.

### Duplicate `NullModel` Removed (resolved)
All code imports `react_core::llm::NullModel`.

### Legacy `config_from_env` Removed (resolved)
Replaced by `config_from_global()`.

### Suite-Owned LLM Calls Are First-Class Events (resolved)
All suite LLM calls go through `AgentCtx::llm_chat` / `llm_chat_json`.

### Timeout And Retry Policy Unified (resolved)
`LlmCallOptions.timeout_secs` and JSON parse retry unified.

### Domain-Specific Provider Traits Moved to Suite (resolved)
Core retains only generic primitives: `VectorStore`, `SecretsProvider`, `StateStore`.

### Suite-Specific Modules Relocated (resolved)
Provider crates moved to `data_engineer/modules/`. Generic modules remain at `src/modules/`.

### Keyspace Trait Genericized (resolved)
Core `Keyspace` reduced to `scoped_key()` + `scoped_prefix()`.

### `control_flow.rs` Removed from Core (resolved)
Domain types moved to suite. Generics introduced for guard kinds.

### Session Types Cleaned (resolved)
String-based serialization boundary. Generic `extensions: Value`. Capability registry.

### Capability Registry on `SuiteCtx` and `AgentCtx` (resolved)
`HashMap<TypeId, Arc<dyn Any>>` capability map. `ctx.capability::<dyn T>()`.
