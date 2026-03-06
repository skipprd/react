# Tech Debt

## `final` Is Overloaded
In core, `final` means the agent has completed its run and produced a terminal result. In `data_engineer` plan discovery, `final.kind="plan_discovery_ready"` is used as a non-terminal handoff into more work. This makes terminal completion and mid-phase readiness representable with the same primitive.

Root cause: the generic agent contract only distinguishes `tool` vs `final`, while plan-phase control flow needs a distinct checkpoint/handoff concept.

Involved code: `src/core/src/schema_registry.rs`, `src/core/src/agent/mod.rs`, `src/core/src/session/materialization.rs`, `src/suites/react-suites/src/prompts/plan.rs`, `src/suites/react-suites/src/data_engineer/phase_plan.rs`.

## Hidden Post-Discovery Planning
`phase_plan` continues substantial work after the terminal-looking `final`: design memo generation, critique, grounding, enrichment, semantic validation, persistence, and auto-advance. This is why logs can appear stalled or finished while the phase is still busy.

Root cause: plan discovery and deterministic plan compilation share one phase implementation, but only the discovery substep is externally visible as an agent step.

Involved code: `src/suites/react-suites/src/data_engineer/phase_plan.rs`, `src/suites/react-suites/src/data_engineer/mod.rs`.

## Control Flow Meaning Is Hidden In `Continue`
`PhaseExecutorOutcome::Continue` collapses multiple distinct meanings into one return path. The outer loop then reloads persisted state and infers what happened indirectly. This weakens compile-time guarantees and makes “stay in phase”, “retry”, “transition committed”, and “annotation only” look too similar in code.

Root cause: control flow is expressed through side effects plus reloaded state rather than through strongly typed executor outcomes.

Involved code: `src/suites/react-suites/src/data_engineer/mod.rs`, `src/suites/react-suites/src/data_engineer/workflow_node.rs`, `src/suites/react-suites/src/data_engineer/transition_dispatcher.rs`.

## Workflow State Is Under-Typed
Workflow routing is derived too heavily from `current_phase`, even though actual legality depends on richer state such as plan revision pending, repair loopback intent, plan bootstrap completion, plan approval status, and repair mode. This leaves important workflow distinctions outside the type system.

Root cause: the workflow cursor is flatter than the real lifecycle.

Involved code: `src/suites/react-suites/src/data_engineer/workflow_node.rs`, `src/suites/react-suites/src/data_engineer/progress_controller.rs`.

## Repair And Planning Intents Are Coupled
`PendingLoopbackIntent` stores both patch-implementation and plan-revision intents in repair state. That couples unrelated workflow concerns and makes correctness depend on every phase remembering to clear or consume the same mutable slot correctly.

Root cause: one persisted intent bucket is serving multiple control-flow roles.

Involved code: `src/suites/react-suites/src/data_engineer/progress_controller.rs`, `src/suites/react-suites/src/data_engineer/phase_review.rs`, `src/suites/react-suites/src/data_engineer/phase_plan.rs`.

## Plan Lifecycle Typing Is Incomplete
The codebase already has useful wrappers such as grounded and persistable plans, but the main plan pipeline still works mostly with raw mutable plan structs plus runtime status checks. That allows invalid lifecycle states to remain representable for too long.

Root cause: typed plan wrappers exist, but they are not yet the primary control-flow boundary.

Involved code: `src/suites/react-suites/src/data_engineer/plan_types.rs`, `src/suites/react-suites/src/data_engineer/plan_grounding.rs`, `src/suites/react-suites/src/data_engineer/phase_plan.rs`, `src/suites/react-suites/src/data_engineer/phase_author.rs`.

## Cleanse And Model Pipelines Drift (partially addressed)
Plan approval, semantic validation retry, and design review stamping have been unified across tracks. `approve_plan_draft_and_advance` now dispatches on `TrackKind` via `TrackPlanDoc`, `handle_plan_semantic_failure` and `stamp_design_review` remove duplicated blocks in `phase_plan.rs`, and dead generic-dispatch types (`TrackSpec`, `CleanseSpec`, `ModelSpec`) have been removed in favour of runtime `TrackKind` dispatch.

Remaining drift: the grounding/compile/enrich/prune pipelines in `phase_plan.rs` still have parallel cleanse vs model branches (~250 lines each) because the data sources and plan shapes differ fundamentally (raw datasets vs staging models, skeleton vs candidates). The `phase_author.rs` authoring loop has a similar parallel structure for plan loading, next-action resolution, and batch-state mapping. These could be further unified via a track-parameterised pipeline abstraction, but the per-track deltas are large enough that the complexity trade-off needs care.

Involved code: `src/suites/react-suites/src/data_engineer/phase_plan.rs`, `src/suites/react-suites/src/data_engineer/plan_review_helpers.rs`, `src/suites/react-suites/src/data_engineer/phase_author.rs`.

## Suite-Owned LLM Calls Are Not First-Class Runtime Events (resolved)
All suite LLM calls now go through `AgentCtx::llm_chat` / `llm_chat_json`, which emit `LlmStart`/`LlmEnd`/`LlmCall` thread events automatically. Zero direct `ctx.llm.chat(...)` calls remain in suites. Dead code removed: `record_llm_call_observability`, per-callsite `llm_calls_enabled()` checks, manual `ThreadStep::LlmCall` construction, duplicate `escape_control_chars_in_json_strings`, dead `LlmSession`/`chat_strict`, and `LlmCallOptions::new`.

## Runtime Timeout And Retry Policy Is Fragmented (resolved)
`LlmCallOptions.timeout_secs` provides per-call timeout support via `AgentCtx::llm_chat`. JSON parse retry with escape-repair is unified in `llm_chat_json`. Suite-local observability and retry code has been removed from `review_batched.rs`, `patch_protocol.rs`, and `remediate.rs`. `openai_compat.rs` timeout default aligned to 1200s to match `router.rs`.
