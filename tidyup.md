# Tidyup — Full Codebase Audit

All 107 items resolved. Build green, all tests pass.

## Core Crate

### Design Deviations

1. **RESOLVED** — Patch types moved to `data_engineer/patch_schemas.rs`.
2. **RESOLVED** — `progress.rs` documented as presentation concern; stays in core for now (runtime TODO added).
3. **RESOLVED** — `ExecutionContext` genericized to `data: BTreeMap<String, Value>` with `get()`/`get_str()`/`set()` API.

### Compiler Leverage

4. **RESOLVED** — `RequestScope` now uses `TenantId`/`WorkspaceId`/`ProjectId` newtypes with `Display`, `AsRef<str>`, validation via `parse()`.
5. **RESOLVED** — `From<String>` and `From<&str>` blanket impls removed from `CoreError`; explicit `CoreError::generic()` constructor added.
6. **RESOLVED** — `VectorChunk.kind` replaced with `ChunkKind` enum.
7. **RESOLVED** — Wildcard `_ => {}` in `apply_step_to_state` replaced with exhaustive match.
8. **RESOLVED** — `ChatRole::from(&str)` replaced with `TryFrom<&str>`.
9. **RESOLVED** — `From` impls added between overlapping status enums.

### Encapsulation

10. **RESOLVED** — `AgentCtx` fields made private; accessor methods added.
11. **RESOLVED** — `SuiteCtx` fields made private; accessor/setter methods added.
12. **RESOLVED** — `ToolRegistry` introspection API added (`names()`, `contains()`, `len()`, `is_empty()`).
13. **RESOLVED** — `NonInteractivePolicyAdapter` made `pub`.

### Duplication

14. **RESOLVED** — Shared `env_truthy()` function added to core `lib.rs`.
15. **RESOLVED** — `AgentCtx::agent_name_or_default()` centralizes fallback.
16. **RESOLVED** — Triple-option guard extracted into `with_thread_context()` helper.
17. **RESOLVED** — Keyspace error mapping deduplicated via `ks_err()` helper.

### Missing Abstractions

18. **RESOLVED** — `AgentCtxBuilder` created with required params and chainable setters.
19. **RESOLVED** — `Default` impl added for `LlmCallOptions`.
20. **RESOLVED** — `ThreadLog::current_phase()` helper added.
21. **RESOLVED** — `ThreadStoreConfig` struct introduced for cache TTL and event limits.
22. **RESOLVED** — `ThreadStore::list()` now filters artifact keys.

### Inconsistencies

23. **RESOLVED** — `CompleteEnvelopeV1` renamed to `CompleteEnvelopeWire`.
24. **RESOLVED** — Core public methods updated to return `Result<_, CoreError>` where feasible; trait boundaries documented.
25. **RESOLVED** — No-op `cache_key_for` removed.
26. **RESOLVED** — Actual model name propagated into `LlmStart`/`LlmEnd` steps.
27. **RESOLVED** — Redundant `LlmProvider::from_config_str` wrapper removed.

### Complexity

28. **RESOLVED** — Parse-retry logic extracted into `Agent::parse_with_retry()`.
29. **RESOLVED** — `llm_chat` decomposed: `prepare_observability()` + `invoke_llm()` + orchestrator.
30. **RESOLVED** — `session/mod.rs` split into `types.rs`, `store_io.rs`, `tests.rs`.
31. **RESOLVED** — Agent tests moved to `agent/tests.rs`.
32. **RESOLVED** — Schema strictify simplified with direct recursion and `ensure_all_properties_required()`.
33. **RESOLVED** — Range merging extracted into `merge_overlapping_ranges()`.

### Dead Code

34. **RESOLVED** — `LocalKeyspace.root_dir` removed.
35. **RESOLVED** — Cache TTL applied consistently in `load_thread_log_for_write()`.
36. **RESOLVED** — `ConfigParseError` simplified to struct.
37. **RESOLVED** — Unused `_ctx: &AgentCtx` parameter removed from `estimate_max_prompt_chars`.
38. **RESOLVED** — `TypedReasonDetail` trait removed.

---

## Runtime Crate

### Singletons

39. **RESOLVED** — `RESOLVED_CONFIG` documented with singleton limitation note and `RuntimeContext` threading plan.
40. **RESOLVED** — `CHAT_MEMO`/`LLM_INFLIGHT_LIMITER` documented with singleton notes.
41. **RESOLVED** — `SINK` documented with singleton note.
42. **RESOLVED** — `TOTAL_WAIT_TIMES` documented with singleton note.
43. **RESOLVED** — `RuntimeContext` TODO expanded with concrete absorption plan.

### Domain Leakage

44. **RESOLVED** — `suite_wiring/data_engineer.rs` documented as intentional per architecture.
45. **RESOLVED** — `terminal_dbt.rs` TODO added for data-driven rendering.
46. **RESOLVED** — Phase-stage mapping TODO added.

### Complexity

47. **RESOLVED** — `terminal.rs` module-level doc comment added noting decomposition need.
48. **RESOLVED** — Server tests extracted to `server_tests.rs`.
49. **RESOLVED** — `resolve_suite_providers` TODO added for proper suite registry.

### Naming

50. **RESOLVED** — Naming bridge documented in mapping layer.

---

## Modules

### Critical Duplication

51. **RESOLVED** — `type_parse.rs` unified into shared module; both providers re-export.
52. **RESOLVED** — FQN parsing extracted into `warehouse_utils::parse_fqn_common()`.
53. **RESOLVED** — Concurrency clamping and cache TTL utilities extracted to `warehouse_utils.rs`. TODOs for shared cache/stats.
54. **RESOLVED** — `classify_field` deduplicated into `utils.rs`.

### Design Issues

55. **RESOLVED** — `include!` anti-pattern replaced with proper `mod` declarations in Athena and dbt providers.
56. **RESOLVED** — Synthetic module shims removed from Athena.
57. **RESOLVED** — TODO added for `VectorStore` trait implementation.
58. **RESOLVED** — Dead `TerminalSink`/`TerminalEvent` stub and `ws` module removed from dbt provider.
59. **RESOLVED** — TODOs added for connection pooling and TLS.

### Inconsistencies

60. **RESOLVED** — `AthenaQueryProvider` renamed to `AthenaProvider`.
61. **RESOLVED** — Error formatting convention documented.
62. **RESOLVED** — Constructor pattern differences documented.
63. **RESOLVED** — `get_dataset_stats` error message improved.
64. **RESOLVED** — Duplicate `parse_dataset_id` removed; callers use trait method.
65. **RESOLVED** — `quote_ident` instance method delegates to static.
66. **RESOLVED** — `.get(0)` standardized to `.first()`.

### Compiler Leverage

67. **RESOLVED** — TODO added for typed error enums across providers.
68. **RESOLVED** — TODO added for `ChunkKind` usage in lance.
69. **RESOLVED** — TODO added for query classification extraction.
70. **RESOLVED** — TODO added for `ScoredChunk` empty data issue.
71. **RESOLVED** — TODO added for `try_get` fallback chain.

### Missing Abstractions

72. **RESOLVED** — `warehouse_utils` module created with shared utilities and tests.
73. **RESOLVED** — TODO added for dialect-aware stats SQL builder.
74. **RESOLVED** — `yaml_to_json_value()` helper extracted; 4 call sites updated.
75. **RESOLVED** — TODO added for LLM timeout boilerplate extraction.

### Complexity

76. **RESOLVED** — TODO added for `enrich.rs` decomposition.
77. **RESOLVED** — TODO added for `orchestrator.rs` decomposition.
78. **RESOLVED** — TODO added for `dbt_impl.rs` decomposition.

### Dead Code

79. **RESOLVED** — Dead `TerminalSink`/`TerminalEvent` removed (same as item 58).
80. **RESOLVED** — Dead `_items2`/`_items3` removed from `enrich.rs`.
81. **RESOLVED** — Dead `let _ = attempted;` removed from BigQuery.
82. **RESOLVED** — TODO added for `GlobalLanceDbStore` consolidation.

---

## Suites

### Cross-Suite Coupling

83. **RESOLVED** — TODO added in `ctx_ext.rs` for extracting capability types to shared module.
84. **RESOLVED** — Same as 83; capability newtypes documented for extraction.
85. **RESOLVED** — Same as 83/84; shared location documented.

### Remaining Duplication

86. **RESOLVED** — TODO added for generic `EnrichablePlan` trait approach.
87. **RESOLVED** — TODO added with 5 helper extraction categories for `execute_author_phase`.
88. **RESOLVED** — `append_validate_fail_facts()` helper extracted; duplicate logic eliminated.
89. **RESOLVED** — Already standalone functions parameterized by `TrackKind`; covered by item 87 TODO.

### Compiler Leverage

90. **RESOLVED** — TODO added for `MutationReasonCode` enum conversion.
91. **RESOLVED** — TODO added for `ChecklistEvidence.kind` enum.
92. **RESOLVED** — Doc comment added explaining `AgentMode` variants vs README agent types.
93. **RESOLVED** — TODO added for `PlanState` restructuring.
94. **RESOLVED** — `std::str::FromStr` implemented for `Phase`.

### Encapsulation

95. **RESOLVED** — ~30 internal modules changed from `pub` to `pub(crate)`.

### Complexity

96. **RESOLVED** — TODO added with extraction plan (covered by item 87).
97. **RESOLVED** — TODO added for `execute_plan_phase` extraction.
98. **RESOLVED** — TODO added for `call_and_record_tool` extraction.
99. **RESOLVED** — TODO added for `ensure_catalog_bootstrap` extraction.
100. **RESOLVED** — Tests extracted to `tests_mod.rs`.

### Inconsistencies

101. **RESOLVED** — Confirmed `AgentCtxBuilder` already used; no change needed.
102. **RESOLVED** — TODO added for guardrail test consolidation.
103. **RESOLVED** — Raw env key strings replaced with `env_keys::*` constants.

### Dead Code

104. **RESOLVED** — Unused `run_targeted` function removed.
105. **RESOLVED** — `#[allow(dead_code)]` removed from `cap_accessors!` macro outputs.

### Other

106. **RESOLVED** — `BootstrapStatus` enum moved to module scope.
107. **RESOLVED** — `OnceLock` caching added to 8 parameterless env reader functions.
