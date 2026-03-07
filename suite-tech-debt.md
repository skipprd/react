# Suite Tech Debt — `data_engineer`

All 165 previously identified items have been resolved. No open suite tech debt remains.

---

## Resolved

### Generic `Plan<T: PlanTask>` Introduced (resolved — items 23-51, 161-162)
`CleansePlan` and `ModelPlan` are now type aliases for `Plan<CleanseTask>` and `Plan<ModelTask>`. `PlanTask` trait abstracts over task identifier. Eliminated ~40 cleanse/model duplication items: function pairs in `plan_progress.rs`, `plan_validation.rs`, `plan_grounding.rs`, and newtype wrappers are all generic. `TrackPlan` impl is generic on `Plan<T>`.

### `PlanKind`/`TrackKind` Unified (resolved — items 35-37, 73)
`PlanKind` is now a type alias for `TrackKind`. Single canonical enum with serde support and phase mapping methods.

### Dead Code Removed (resolved — items 75-93)
Unreachable targeted-validate section, duplicate `save_*_plan_grounded` calls, dead `AgentMode` variants, trivial wrapper functions, unused detail structs, no-op preflight stubs, dead type aliases, `ProvidersResolvedSerde` intermediaries all removed.

### Tiny Modules Inlined (resolved — items 146-152)
`providers/limits.rs`, `types.rs`, `project_files.rs`, `util/dedup.rs`, `util/time_context.rs`, `preflight/catalog_preflight.rs` contents moved to their consumers and files deleted.

### `DeterministicDbtValidateOnce`/`Targeted` Unified (resolved — item 52)
Merged into single struct with `run()` and `run_targeted()` delegating to `run_inner()`.

### Extraction Helpers Deduplicated (resolved — item 53)
Shared `extract_string_vec_from_extra` replaces 3 duplicate blocks.

### `run_ask`/`run_review` Unified (resolved — item 54)
Extracted `run_outcome_to_frames()` for shared outcome handling.

### `handle_new`/`handle_open`/`handle_user` Unified (resolved — item 55)
Extracted `dispatch_agent()` shared dispatch helper.

### AgentCtx Builder (resolved — items 56-57)
`build_agent_ctx()` replaces 4 duplicated construction sites.

### Env Var Parsing Deduplicated (resolved — items 58, 68)
`env_util::max_replan_backtracks()` is the single source. All env var parsing consolidated into `env_util.rs`.

### Phase Ordinal (resolved — item 59)
`Phase::ordinal()` method and `ALL_PHASES` constant replace inline closures.

### `extract_source_calls` Unified (resolved — item 60)
Single canonical implementation in `naming.rs`, re-exported by `facts.rs`.

### `parse_quoted` Unified (resolved — item 61)
Single implementation in `naming.rs` with escape handling.

### `is_runnable_checklist_status` Consolidated (resolved — item 62)
Defined once in `plan_progress.rs`, copies removed from other files.

### `ensure_checklist_item` Unified (resolved — item 63)
Single function with label parameter.

### Redundant Re-grounding Eliminated (resolved — item 64)
Grounding results passed through in `approve_plan_draft_and_advance`.

### Publish Retry Pattern Extracted (resolved — item 65)
`check_publish_retry_limit` helper replaces 3 repeated blocks.

### `ctx_ext.rs` DRYed with Macro (resolved — items 66, 155)
`cap_accessors!` macro generates all 12 functions. Doc comment lists 5-place checklist for new providers.

### Status Mapping Simplified (resolved — item 67)
Generic `serde_str()` using `serde_json::to_value()` replaces 5 manual match functions.

### Diff Functions Share Single Pass (resolved — item 69)
`DiffResult` + `diff_lines_inner()` shared by `compute_unified_diff` and `diff_stats`.

### State Manager Validation Extracted (resolved — item 70)
Shared `validate_loaded_state` used by both load functions.

### `as_str` Round-Trip Tests Added (resolved — items 71-72)
`domain_types.rs` and `control_flow.rs` have round-trip tests for all `as_str`/`from_str` conversions.

### Preflight Provider Check Fixed (resolved — item 19/66)
Consolidated two `sctx_dbt` calls into single let-else.

### Review Persistence Extracted (resolved — items 48-49/20-21)
`mutate_plan_review` generic helper + `load_plan_and_batches` extracted.

### RepairModeCore Extracted (resolved — item 22/46-47)
Shared struct with `#[serde(flatten)]` embedded in both repair mode types.

### All Type Safety Items Fixed (resolved — items 94-114)
- `ModelTask.folder` → `ModelFolder` enum
- `JoinSpec.join_type` → `JoinType` enum
- `JoinSpec.cardinality` → `Cardinality` enum
- `PlanMutation.reason_code` → `MutationReasonCode` newtype
- `PlanDesignBlockerV1.severity` → `Severity` enum
- `phase_reason_detail` strings → proper enums/newtypes
- `FactsBundle.scope` → `FactsScope` enum, `dialect` → `SqlDialect` newtype
- `WarehouseNaming::kind()` → returns `WarehouseKind` enum
- `PromptEnvelope.phase` → `Phase` enum
- `WsPlanKind` removed, uses `TrackKind`
- `PLAN_SPEC_PLACEHOLDER_SENTINEL` → fields are `Option<T>`
- `AgentMode::parse` → `FromStr` impl
- Bootstrap status → `BootstrapStatus` enum
- `resolve_warehouse` no longer round-trips through JSON

### God Modules Split (resolved — items 1-22)
- `mod.rs` 4182→2075 lines: extracted `enrichment.rs`, `catalog_bootstrap.rs`, `agent_modes.rs`, `llm_profiles.rs`
- `phase_author.rs`: extracted `resolve_checklist_item_id`, `build_schema_checklist_context`, `check_batch_lock_and_loopback`, `extract_validate_fail_context`
- `phase_plan.rs`: extracted `run_plan_bootstrap`
- `review_batched.rs` 2104→1550 lines: extracted `review_prompts.rs`, `review_persistence.rs`
- `project_fs/mod.rs` 2181→466 lines: extracted `patch.rs`, `yaml.rs`, `diff.rs`
- `tool_registry_builder.rs` 821→432 lines: extracted `tool_policies.rs`

### Magic Values Extracted (resolved — items 115-126)
- LLM profiles → `LlmProfile` struct + named constants
- Tool timeouts → `TOOL_TIMEOUT_FAST/MEDIUM/SLOW/EXTRA_SLOW_SECS`
- Agent context → `DEFAULT_TOP_K`, `ASK_MAX_STEPS`, `REVIEW_MAX_STEPS`, etc.
- Project identity → `SUITE_PROJECT_NAME`, `DEFAULT_AGENT_NAME`, `UNKNOWN_AGENT`
- File limits → `FILE_LIST_LIMIT`, `FILE_GET_MAX_CHARS`

### Config Coupling Centralized (resolved — items 127-132)
- `env_util.rs` expanded with `env_keys` module (40+ named constants)
- Centralized reader functions with defaults and clamping
- All 25+ scattered `std::env::var()` calls replaced

### Visibility Restricted (resolved — items 133-137)
- `de_config.rs` types → `pub(crate)`
- `control_flow.rs` functions → `pub(crate)`
- `resolved_config_from_ctx` → `pub(crate)`

### Error Handling Improved (resolved — items 138-145)
- `invariant_has_any_models` logs warning on failure
- `load_execution_state` logs warning on corrupt state
- String-matching documented with rationale
- `.unwrap()` → `.expect()` with context
- Silent fallbacks get `tracing::warn!`
- Double `.to_string()` simplified
- Clone-and-replace replaced with direct mutation
- `phase_reason_detail` structs derive `Deserialize`

### Architecture Improved (resolved — items 153-165)
- Thread cache has eviction at 1000 entries
- `DatasetRef`/`DatasetId` have `From` impls between them
- `copy_capabilities_to_actx` re-exported as public API
- Domain types consolidated in `domain_types.rs`
- Guard block messages use structured format
- Plan schema types deduplicated via `JsonSchema` on canonical types
- Staging/gold model shared `model_authoring_engine.rs`
- `TrackPlanDoc` delegation via `delegate_track_plan!` macro
- `plan_storage.rs` extracted from `plan_progress.rs`
- Dead code warnings suppressed on intentionally-retained macro-generated items
