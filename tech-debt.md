# Tech Debt

## Runtime Crate

### `ws/server.rs` Is Still 2760 Lines

The initial decomposition extracted utilities, mapping, connection state, thread state, suite runner, and history into submodules. However `server.rs` still contains the WS listener/accept loop, `handle_message` dispatch (~940 lines), five `process_*` request handler functions (~400 lines), `run_headless_with_hub` (~130 lines), and ~1000 lines of tests. A second pass should extract `process_new`/`process_open`/`process_user`/`process_approve`/`process_reject` into a `handlers.rs` submodule and move `handle_message` into a `protocol.rs` dispatcher.

Involved code: `src/runtime/src/ws/server.rs`.

### `Final` Terminology in WS API and OpenAPI Models

The WS API layer uses `Final` extensively (`FinalResult`, `AskFinalPayload`, `FinalResponse`, `ws_final_result_from_typed_final`, etc.). The `api_gen/` directory has 87+ generated files referencing `Final`. This is the external API boundary driven by `openapi/ws-core.yaml`, so renaming requires a coordinated OpenAPI spec update, code regeneration, and client migration. It is inconsistent with core's `Complete` terminology.

Root cause: the OpenAPI spec was authored before the `final` -> `complete` rename in core.

Involved code: `src/runtime/src/ws/server.rs`, `src/runtime/src/ws/api_gen/`, `runtime/openapi/ws-core.yaml`.

### Process-Global Singletons Impair Testability

`RESOLVED_CONFIG: OnceCell` (runtime_settings.rs), `SCOPE_PREFERENCE: OnceLock` (runtime_settings.rs), `CHAT_MEMO: OnceCell<DashMap>` (router.rs), `LLM_INFLIGHT_LIMITER: OnceCell` (router.rs), `SHARED_LOCAL: OnceCell` (llama_cpp.rs), and `SINK: OnceCell` (terminal.rs) are all process-global. The runtime can only serve one config per process and tests that touch these singletons require env-var mutex serialization.

Root cause: config and model state was initialized once at startup before multi-tenant or test-parallel needs arose.

Involved code: `src/runtime/src/runtime_settings.rs:5,100`, `src/runtime/src/llm/router.rs:62,153`, `src/runtime/src/llm/llama_cpp.rs:127`, `src/runtime/src/ws/terminal.rs:50`.

### `getenv_nonempty` Is Still Duplicated

The same `getenv_nonempty` helper is defined in both `config.rs` (line 237) and `bootstrap.rs` (line 18). They have identical semantics.

Root cause: `bootstrap.rs` was extracted from `main.rs` and brought its own copy; `config.rs` retained its independent definition.

Involved code: `src/runtime/src/config.rs:237`, `src/runtime/src/bootstrap.rs:18`.

### `ws/terminal.rs` Is 2037 Lines

The terminal rendering module handles crossterm TUI layout, event processing, spinner animation, and all the rendering logic in a single file. This is larger than several of the extracted WS submodules combined. It could benefit from decomposition into layout, rendering, and event handling submodules.

Involved code: `src/runtime/src/ws/terminal.rs`.

---

## Suite Crate

### `complete` Envelope Is Overloaded as a Non-Terminal Handoff

In core, `complete` means the agent has finished and produced a terminal result. In `data_engineer` plan discovery, `complete.kind="plan_discovery_ready"` is used as a non-terminal handoff into deterministic post-processing. Terminal completion and mid-phase readiness are representable with the same primitive.

Root cause: the generic agent contract only distinguishes `tool` vs `complete`, while plan-phase control flow needs a distinct checkpoint/handoff concept.

Involved code: `src/core/src/schema_registry.rs`, `src/core/src/agent/mod.rs`, `src/core/src/session/materialization.rs`, `src/suites/react-suites/src/prompts/plan.rs`, `src/suites/react-suites/src/data_engineer/phase_plan.rs`.

### Hidden Post-Discovery Planning

`phase_plan` continues substantial deterministic work after the agent emits `complete`: design memo generation, critique, grounding, enrichment, semantic validation, persistence, and auto-advance. Logs can appear stalled or finished while the phase is still busy.

Root cause: plan discovery and deterministic plan compilation share one phase implementation, but only the discovery substep is externally visible as an agent step.

Involved code: `src/suites/react-suites/src/data_engineer/phase_plan.rs`, `src/suites/react-suites/src/data_engineer/mod.rs`.

### Control Flow Meaning Is Hidden In `Continue`

`PhaseExecutorOutcome` has only `Continue` and `Return(Vec<FlowFrame>)`. `Continue` collapses "stay in phase", "retry", "transition committed", and "annotation only" into one return path. The outer loop reloads persisted state and infers what happened indirectly, weakening compile-time guarantees.

Root cause: control flow is expressed through side effects plus reloaded state rather than through strongly typed executor outcomes.

Involved code: `src/suites/react-suites/src/data_engineer/mod.rs:175-178`, `src/suites/react-suites/src/data_engineer/transition_dispatcher.rs`.

### Plan Lifecycle Typing Is Incomplete

The codebase has `TrackPlan` trait and typed plan wrappers, but the main plan pipeline still works mostly with raw mutable plan structs plus runtime status checks. Invalid lifecycle states remain representable.

Root cause: typed plan wrappers exist, but they are not yet the primary control-flow boundary.

Involved code: `src/suites/react-suites/src/data_engineer/plan_types.rs`, `src/suites/react-suites/src/data_engineer/plan_grounding.rs`, `src/suites/react-suites/src/data_engineer/phase_plan.rs`, `src/suites/react-suites/src/data_engineer/phase_author.rs`.

### Cleanse And Model Pipelines Drift (partially addressed)

Plan approval, semantic validation retry, and design review stamping have been unified across tracks via `TrackKind` dispatch. Remaining drift: the grounding/compile/enrich/prune pipelines in `phase_plan.rs` still have parallel cleanse vs model branches because the data sources and plan shapes differ fundamentally. The `phase_author.rs` authoring loop has a similar parallel structure.

Involved code: `src/suites/react-suites/src/data_engineer/phase_plan.rs`, `src/suites/react-suites/src/data_engineer/plan_review_helpers.rs`, `src/suites/react-suites/src/data_engineer/phase_author.rs`.

---

## Resolved

### Suite-Specific Logic Extracted from Runtime (resolved)
`providers/catalog/` (2115 lines), `embeddings/mod.rs` (407 lines), and `providers/type_parse.rs` moved to suite-owned module crates under `data_engineer/modules/`. Suite-specific re-exports removed from `providers/mod.rs`. `global_dbt_examples_store` removed from `LanceVectorStore`. `DbtProgress` terminal event replaced with generic `SubprocessProgress`.

### `main.rs` Provider Wiring Deduplicated (resolved)
`build_suite_ctx()` extracted to `bootstrap.rs`. Both `Serve` and `Run` call the shared builder. `main.rs` reduced from ~1228 to ~803 lines.

### `config.rs` Decoupled from `data_engineer` Suite (resolved)
All `react_suites::data_engineer::de_config` imports removed. The `providers:` YAML section is parsed as opaque `serde_json::Value` and passed through `suite_config`. Suite-specific config types live in the suite.

### Dual Config Access Paths Unified (resolved)
`helpers/configuration.rs` deleted. `runtime_settings.rs` is the single canonical config source. All LLM files (`llm/mod.rs`, `llm/router.rs`, `llm/session.rs`, `llm/llama_cpp.rs`, `llm/llama_cpp_adapter.rs`, `llm/openai_compat.rs`) migrated to `runtime_settings::*`. Legacy `config_from_env()` replaced with `config_from_global()`.

### `ws/server.rs` Partially Decomposed (resolved)
Reduced from 4694 to 2760 lines. Extracted `ws/util.rs` (96), `ws/mapping.rs` (113), `ws/conn_state.rs` (206), `ws/thread_state.rs` (493), `ws/suite_runner.rs` (893), `ws/history.rs` (114).

### Re-Export Shim Modules Removed (resolved)
Deleted: `adapters/storage.rs`, `helpers/progress.rs`, `discover/` (mod.rs + stats.rs), `providers/state.rs`, `providers/scope.rs`, `providers/dataset_catalog_provider.rs`, `providers/query.rs`, `util/dedup.rs`. Entire `adapters/` and `discover/` directories removed.

### `lazy_static!` Replaced (resolved)
`timed_rwlock.rs` now uses `std::sync::LazyLock`. `PROFILE_PERFORMANCE` is configurable via `REACT_PROFILE_LOCKS` env var (default: off). `lazy_static` dependency removed from `Cargo.toml`.

### `CHAT_MEMO` Capped (resolved)
`CHAT_MEMO` in `llm/router.rs` now has a 256-entry cap with TTL-based eviction. Stale entries are purged on each insert.

### Duplicate `NullModel` Removed (resolved)
Runtime-local `NullModel` deleted. All code imports `react_core::llm::NullModel`.

### Legacy `config_from_env` Removed (resolved)
`config_from_env()` deleted from `llm/mod.rs`. Replaced by `config_from_global()` which reads from the resolved config.

### Suite-Owned LLM Calls Are First-Class Runtime Events (resolved)
All suite LLM calls now go through `AgentCtx::llm_chat` / `llm_chat_json`, which emit `LlmStart`/`LlmEnd`/`LlmCall` thread events automatically.

### Runtime Timeout And Retry Policy Unified (resolved)
`LlmCallOptions.timeout_secs` provides per-call timeout support via `AgentCtx::llm_chat`. JSON parse retry with escape-repair is unified in `llm_chat_json`.

### Domain-Specific Provider Traits Moved to Suite (resolved)
`QueryProvider`, `DbtProvider`, `CatalogProvider`, `WarehouseProvider`, `DatasetCatalogProvider` and associated types moved from `react-core` to `react_suites::data_engineer::providers/`. Core retains only generic primitives: `VectorStore`, `SecretsProvider`, `StateStore`.

### Suite-Specific Modules Relocated (resolved)
`provider-athena`, `provider-bigquery`, `provider-postgres`, `provider-dbt` moved from `src/modules/` to `src/suites/react-suites/src/data_engineer/modules/`. Generic modules (`storage`, `provider-vector-lance`) remain at `src/modules/`.

### Keyspace Trait Genericized (resolved)
Core `Keyspace` trait reduced to `scoped_key()` + `scoped_prefix()` with default methods for threads/logs. All domain-specific key methods (`catalog_key`, `dbt_prefix`, `lancedb_uri`, etc.) removed from core.

### `control_flow.rs` Removed from Core (resolved)
`ReviewDecision`, `PhaseReasonCode`, `GuardBlockKind` moved to `data_engineer/domain_types.rs`. `PhaseDirective` and `PreTurnDirective` genericized over guard kind. `WorkflowSuiteContract` gained `GuardKind` associated type.

### Session Types Cleaned (resolved)
`CompleteKind` enum replaced with `String`. `ThreadStep::Phase.reason_code` and `ThreadStep::GuardBlock.kind` are now `String` at the serialization boundary. `ThreadBootstrapState.catalog` replaced with generic `extensions: serde_json::Value`. `ArtifactKind` enum replaced with `String`. `ThreadCache`/`ThreadCacheStore` moved to suite.

### Capability Registry on `SuiteCtx` and `AgentCtx` (resolved)
Domain-specific fields (`warehouse`, `dbt`, `query`, `datasets`, `catalog`) replaced with typed capability map (`HashMap<TypeId, Arc<dyn Any + Send + Sync>>`). Suites retrieve providers via `ctx.capability::<dyn T>()`.
