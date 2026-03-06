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

## Suite-Owned LLM Calls Are Not First-Class Runtime Events
Many suite paths call `ctx.llm.chat(...)` directly. Those calls do not consistently participate in the same visibility model as the core agent loop, so long-running LLM work can be real but look idle in thread history and UI projections.

Root cause: there is no single suite-owned observed LLM wrapper that enforces prompt identity, phase/substage context, start/end events, and shared timeout behavior.

Involved code: `src/suites/react-suites/src/data_engineer/mod.rs`, `src/core/src/agent/run_loop.rs`, `src/core/src/session/projection.rs`, `src/runtime/src/llm/router.rs`.

## Runtime Timeout And Retry Policy Is Fragmented
LLM timeout and retry behavior is split across runtime layers and suite-local loops. This makes latency and apparent stalls harder to reason about and leads to inconsistent observability across call sites.

Root cause: transport policy, blocking behavior, and local retry logic are not funneled through one shared path.

Involved code: `src/runtime/src/llm/router.rs`, `src/runtime/src/llm/openai_compat.rs`, `src/suites/react-suites/src/data_engineer/review_batched.rs`, `src/suites/react-suites/src/data_engineer/patch_protocol.rs`, `src/suites/react-suites/src/data_engineer/dbt_repair/remediate.rs`.
