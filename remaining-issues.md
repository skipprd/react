# Remaining Issues

Only open items are listed here. The earlier cleanse/model plan-pipeline issues have been fixed and are intentionally omitted.

The remaining work is grouped by priority and ordered to favor the simplest design simplifications first.

---

## P0 Critical

### 1. Storage reads still collapse failure into absence

**Files**: `src/core/src/session/store_io.rs`, `src/core/src/session/control_state.rs`

This is the most dangerous remaining issue. Some read paths still blur these two cases:

- the value is genuinely missing
- storage failed to return the value

That allows transient storage failure to be treated as `empty thread` or `no control state`, which can silently destroy continuity and cause the system to continue from bad premises.

**Why this matters**
- Breaks the design goal that failures must be observable, not reinterpreted as normal absence.
- Makes data loss representable.
- Undermines every higher-level retry/guard mechanism because the underlying state may already be wrong.

**Best simplification**
- Split `missing` from `read failed` in the API.
- Keep transport/storage failure in `Result`.
- Represent presence with a small enum.

```rust
enum LoadState<T> {
    Missing,
    Loaded(T),
}

fn load_thread_log(...) -> Result<LoadState<ThreadLog>, CoreError>;
fn load_control_state(...) -> Result<LoadState<ExecutionState>, CoreError>;
```

**Compiler benefit**
- Callers can no longer accidentally treat read failure as empty/default state.
- Mutation helpers can require `Loaded(T)` and make `continue anyway` impossible.

### 2. Read-modify-write races can still silently lose updates

**Files**: `src/core/src/session/store_io.rs`, `src/core/src/session/control_state.rs`

Even when reads succeed, the current mutation flow still permits lost updates via read-modify-write races.

**Why this matters**
- Thread steps or control-state transitions can be overwritten without an explicit error.
- Violates the assumption that state transitions are durable and monotonic.

**Best simplification**
- Add versioned reads and conditional writes.
- Make conflicts explicit instead of silent.

```rust
load(version) -> mutate -> save_if_version_matches(version)
```

**Compiler benefit**
- Callers must handle `Conflict` explicitly instead of silently clobbering newer state.

---

## P1 High

### 3. `StayInPhase` is still too weak and allows no-op loops

**File**: `src/suites/data_engineer/src/phase_author.rs`

There is still a path where authoring can return without tool calls or a committed transition and the controller loops back into the same phase. That is no longer a silent exception, but it is still a silent no-progress loop.

**Why this matters**
- Burns budget with no observable forward movement.
- Makes `no progress` representable as ordinary control flow.

**Best simplification**
- Strengthen the phase outcome contract so `stay` must say why.

```rust
enum PhaseExecutorOutcome {
    TransitionCommitted,
    Return(Vec<FlowFrame>),
    StayedWithProgress { detail: String },
    StayedWaiting { reason: String },
    Failed { reason: String },
}
```

**Compiler benefit**
- A phase can no longer return a vague `StayInPhase`.
- Callers must handle `progress`, `waiting`, and `failed` separately.

### 4. Run-loop stop reasons are not fully centralized or recorded

**File**: `src/core/src/agent/run_loop.rs`

The audit identified two remaining gaps:

- rejected `Complete` actions can burn budget without a first-class stop record
- step-limit exhaustion can exit without a clear terminal thread event

**Why this matters**
- Threads can end or stall without a complete causal record.
- Observability remains partial at the most important control seam.

**Best simplification**
- Centralize all run-loop exits through one recorder.
- Model stop reasons as a typed enum.

```rust
enum RunLoopStop {
    RejectedComplete { reason: String },
    StepLimitExceeded,
    PolicyBlocked { reason: String },
}
```

**Compiler benefit**
- Every exit path must choose an explicit stop reason.
- No more bare returns that bypass recording.

### 5. Panic paths can still orphan tool observability

**File**: `src/core/src/session/observed.rs`

`run_observed` fixed the normal unmatched `ToolStart` problem, but panic/unwind paths can still leave partially written tool observability.

**Why this matters**
- The contract is still not total.
- Tool activity can look incomplete in exactly the cases where the trace matters most.

**Best simplification**
- Either make observed execution unwind-safe with a guard/finalizer, or
- explicitly record outer-boundary abort/failure as the canonical fallback.

**Compiler benefit**
- Less about the type system here, more about enforcing a single construction path where partial tool traces are impossible in normal control flow.

---

## P2 Medium

### 6. Enrichment flow is still duplicated and can drift again

**File**: `src/suites/data_engineer/src/enrichment.rs`

The main remaining suite-level design smell is duplicated enrichment orchestration. The earlier ordering/grounding issues are fixed, but the two enrichment flows are still structurally near-identical.

**Why this matters**
- Future fixes can drift again.
- Complexity remains higher than necessary.

**Best simplification**
- Extract a generic `enrich_tasks<T: EnrichablePlan>()`.
- Keep only track-specific schema/spec application details in trait methods.

```rust
trait EnrichablePlan: PlanTask {
    type Spec;
    type EnrichmentItem;
    fn apply_spec(task: &mut Self, spec: Self::Spec);
    fn is_placeholder(task: &Self) -> bool;
}
```

**Compiler benefit**
- Shared control flow becomes single-source.
- Divergence requires an explicit type-level choice instead of copy/paste drift.

---

## Priority Summary

| Priority | Item | Simplest design move |
|----------|------|----------------------|
| **P0** | Storage reads collapse failure into absence | Typed load API: `Result<LoadState<T>, CoreError>` |
| **P0** | Read-modify-write races lose updates | Versioned/CAS writes with explicit conflict handling |
| **P1** | No-op phase stays are representable | Replace `StayInPhase` with typed stay outcomes |
| **P1** | Run-loop exits are not centrally recorded | Single typed stop-reason recorder |
| **P1** | Panic paths can orphan tool traces | Guard/finalizer or explicit outer abort record |
| **P2** | Enrichment flow duplication | Generic `EnrichablePlan`-based orchestration |
# Remaining Issues — Audit Follow-Up

Primary thread evidence: `3ac65b27`, `3a383d24` (both built SILVER successfully, both died in `model_plan` grounding).

Additional evidence comes from the broader core + data_engineer audit performed after the phase-execution fixes. That audit identified a separate class of remaining issues: storage read paths that silently degrade to "empty/not found", agent/run-loop budget burn with no terminal record, and panic/race paths that still violate the "make dead ends unrepresentable" goal.

---

## Why Cleanse Works But Model Doesn't

The cleanse and model plan pipelines follow structurally different patterns despite sharing the same `Plan<T>`, `GroundedPlan<T>`, and `PlanWorkGroup` types. The asymmetries compound into a failure mode that is **guaranteed** for any non-trivial model plan.

### Cleanse pipeline (phase_plan.rs ~L607-820)

```
catalog → deterministic skeleton → compile plan + work_groups
  → prune tasks/batches against warehouse
  → enrich surviving tasks only
  → persist (GroundedCleansePlan::try_from validates)
  → semantic validation
```

Task IDs come from the catalog. They are grounded in reality by construction. Pruning happens before enrichment, so LLM budget is only spent on tasks that will survive. Work_groups are generated from the pre-prune task set, but since pruning rarely removes catalog-derived tasks, work_groups stay consistent.

### Model pipeline (phase_plan.rs ~L821-1023)

```
LLM → generate_model_candidates → compile plan + work_groups
  → enrich ALL tasks (LLM budget spent on hallucinated tasks)
  → prune tasks/batches against staging models
  → persist (GroundedModelPlan::try_from validates) ← FAILS HERE
  → semantic validation
```

Task IDs come from the LLM. They can include hallucinated names (`validate_*`, aggregate metrics, etc.). Work_groups are generated from ALL candidates before pruning. Pruning removes tasks with bad inputs but does NOT update work_groups. The grounding check then correctly rejects the plan because work_groups reference pruned tasks.

### Summary of the asymmetry

| Step | Cleanse | Model |
|------|---------|-------|
| Task source | Deterministic (catalog) | LLM (hallucination-prone) |
| Prune vs enrich order | Prune first, enrich survivors | Enrich all, prune after |
| Work_group update after prune | Never (but rarely needed) | Never (**always needed**) |
| Grounding check | Passes (tasks match reality) | Fails (stale work_groups) |

---

## Specific Issues

### I1: Pruning never updates work_groups (ROOT CAUSE)

**Files**: `plan_grounding.rs:57-91` (cleanse), `plan_grounding.rs:93-205` (model)

Both prune functions update `plan.tasks` and `plan.batches` but leave `plan.work_groups` untouched. The `validate_plan_structure` function (`plan_validation.rs:218-251`) then catches stale work_group→task references as errors.

This is the direct cause of both thread failures:
- `3ac65b27`: `fct_order_item` pruned → work_groups still reference it
- `3a383d24`: `validate_*` tasks pruned → work_groups still reference them, `model_validate` exceeds 5-item cap

**Why this is allowed by the compiler**: `Plan<T>` stores `tasks`, `batches`, and `work_groups` as independent `Vec`s with no structural invariant linking them. Any code can mutate one without the others.

### I2: Model enriches before pruning, wasting LLM budget

**File**: `phase_plan.rs:903-924`

The model path calls `enrich_model_tasks` (lines 911-919) BEFORE `prune_model_plan_to_grounded_staging_models` (lines 921-924). Every pruned task consumed 2+ LLM calls (reason + compile) for nothing.

In the audited threads, this wasted ~19 LLM calls and ~18 minutes per thread.

The cleanse path does the opposite (prune first at line 619, enrich at line 741), which is correct.

### I3: `save_*_plan_grounded` re-prunes internally, compounding I1

**Files**: `plan_storage.rs:132-144` (cleanse), `plan_storage.rs:184-196` (model)

Both `save_*_plan_grounded` functions clone the plan, re-run pruning, then call `GroundedPlan::try_from`. The caller has already pruned, so this is a redundant prune on a clone. But critically, this second prune also doesn't update work_groups on the clone — so the `try_from` validation sees the same stale work_groups.

This means even if the caller somehow fixed work_groups before calling `save_model_plan_grounded`, the internal re-prune would break them again.

### I4: `GroundedModelPlan::try_from` skips grounding checks

**File**: `plan_grounding.rs:426-441`

```rust
impl TryFrom<ModelPlan> for GroundedModelPlan {
    fn try_from(mut value: ModelPlan) -> Result<Self, Self::Error> {
        ensure_expected_model_paths_model(&mut value);
        let mut errors = strict_model_grounding_errors(&value, None);     // None!
        let sem = validate_model_plan_semantics(&value, None);            // None!
        ...
    }
}
```

Both `strict_model_grounding_errors` and `validate_model_plan_semantics` receive `None` for `allowed_staging_models`, so input grounding checks are skipped. The `Grounded` newtype is supposed to be a compile-time guarantee that the plan is valid — but it doesn't actually check inputs against reality.

### I5: No `canonical_work_groups_from_batches` re-derivation after mutation

**File**: `plan_progress.rs:169-240`

`canonical_work_groups_from_batches` is only called during initial plan compilation (`compile_cleanse_skeleton_plan`, `compile_model_candidates_plan`). After any mutation to tasks or batches (pruning, semantic repair, re-enrichment), work_groups are never regenerated. There is no function that reconciles work_groups against the current task/batch state.

### I6: Enrichment TODO(item-86) — cleanse/model enrichment drift

**File**: `enrichment.rs:167`

`enrich_cleanse_tasks` and `enrich_model_tasks` are ~165 lines each with near-identical structure (chunk → reason LLM → compile LLM → apply → retry → unresolved check). The TODO acknowledges this but the duplication persists. Any fix to one path (e.g. ordering, error handling, retry logic) must be manually replicated in the other.

### I7: Storage read errors still collapse to "empty/not found" (CRITICAL)

These are the most dangerous non-plan issues from the broader audit because they destroy state while looking like ordinary absence.

#### I7.1: Thread log load treats read failure as empty thread

**File**: `src/core/src/session/store_io.rs`

The audited code path in `load_thread_log_for_write` treats a storage read failure as if the thread log simply does not exist yet. That means a transient storage error can lead to the next write reconstructing the thread from an empty log, effectively discarding prior history.

This is a direct violation of the design goal: failure should be observable and represented, not silently converted into "start over".

**Construction fix**:
- Split "missing key" from "storage read failed" in the type system.
- Return a typed result like `enum LoadThreadResult { Missing, Loaded(ThreadLog) }` and keep transport/storage failures in `Result`.
- Make thread mutation APIs require a successfully loaded log, so the compiler prevents "read failed, continue anyway".

#### I7.2: Control-state load treats read failure as missing state

**File**: `src/core/src/session/control_state.rs`

The analogous control-state load path collapses storage failure into "no state present". A transient read error can therefore wipe execution-state continuity and drive incorrect retries, missing loop counters, or lost terminal status.

**Construction fix**:
- Same pattern as thread log loading: `Result<LoadControlState, CoreError>` where `LoadControlState` distinguishes `Missing` from `Loaded`.
- Prohibit mutation/save helpers from accepting an implicit "default state" after a read failure.

### I8: Author phase can stay in phase with no progress mechanism (HIGH)

**File**: `src/suites/data_engineer/src/phase_author.rs`

The audit found a path where authoring can return without tool calls or a committed transition, and the controller simply loops back into the same phase. This is not a silent exception anymore, but it is still a silent no-op loop: step budget burns while the system makes no observable progress.

This is the same class of bug as the earlier validate/author runaway loops, but at a different seam.

**Construction fix**:
- Replace ambiguous `StayInPhase` returns with a typed outcome that distinguishes:
  - `StayedWithProgress`
  - `StayedAwaitingExternalCondition`
  - `FailedNoProgress`
- Require phase handlers to explicitly prove progress when returning a "stay" variant.
- Add a no-progress counter at the phase seam so repeated no-op stays are impossible to ignore.

### I9: Core run loop still burns budget on rejected `Complete` and step-limit exits (HIGH)

**File**: `src/core/src/agent/run_loop.rs`

The audit found two related issues:

#### I9.1: Rejected `Complete` actions consume step budget without a terminal thread event

When the agent emits `Complete` and policy rejects it, the run loop burns step budget and continues. The rejection is not represented as a first-class terminal or failure event in the thread log, so the thread appears to keep working while actually bouncing off policy.

#### I9.2: Step-limit exhaustion stops execution without a terminal thread event

When max-step budget is exhausted, the loop exits but there is no explicit terminal thread step recording "step_limit_exhausted". This leaves users and downstream logic with an incomplete causal record.

**Construction fix**:
- Introduce explicit terminal/nonterminal run-loop outcomes such as:
  - `RunLoopStop::RejectedComplete { reason }`
  - `RunLoopStop::StepLimitExceeded`
  - `RunLoopStop::PolicyBlocked { reason }`
- Require all loop exits to pass through one recorder that appends the terminal/nonterminal stop event before returning.
- Remove any direct bare return from the loop body that bypasses this recorder.

### I10: Read-modify-write races still allow lost updates (HIGH)

**Files**: `src/core/src/session/store_io.rs`, `src/core/src/session/control_state.rs`

The storage/control-state mutation flows still have read-modify-write windows where concurrent writers can overwrite each other's changes. Even when no exception occurs, the contract can silently lose thread steps or state transitions.

This is not just a performance issue; it breaks the "every state transition is durable and observable" assumption.

**Construction fix**:
- Introduce compare-and-swap / etag-based writes or append-only event semantics.
- Represent writes as `load(version) -> mutate -> save_if_version_matches(version)`.
- Make mutation helpers return a conflict outcome that callers must handle explicitly.

### I11: Panic paths can still leave observability half-written (HIGH)

**File**: `src/core/src/session/observed.rs`

`run_observed` centralized tool-call observability and fixed the normal unmatched `ToolStart` problem, but panic paths can still leave a `ToolStart` written without a corresponding `ToolEnd`.

That means the happy-path contract is improved, but not yet total.

**Construction fix**:
- Make observed execution use a guard/finalizer pattern that writes an abnormal `ToolEnd` on unwind.
- If unwind safety is intentionally unsupported, record that via explicit abort handling at the outer boundary so the thread still gains a terminal failure marker.

### I12: Enrichment duplication remains an active drift risk (MEDIUM)

**File**: `src/suites/data_engineer/src/enrichment.rs`

This is now the main remaining cleanse/model drift point inside the plan pipeline. The order-of-operations bug was fixed in `phase_plan.rs`, but the near-identical `enrich_cleanse_tasks` / `enrich_model_tasks` functions still mean future fixes can diverge again.

**Construction fix**:
- Implement the already-documented `EnrichablePlan` trait extraction.
- Make chunking, reason/compile/retry flow, unresolved detection, and snapshot event wiring generic.
- Keep only the genuinely track-specific logic in small trait methods.

---

## Compiler-Enforced Solutions

### S1: Make `Plan<T>` internally consistent by construction

**Problem**: `tasks`, `batches`, and `work_groups` are independent fields. Any code can mutate one without the others, creating silent inconsistency.

**Solution**: Replace the independent `work_groups` field with a derived method. Work_groups are always a deterministic function of `batches` — they should not be stored as separate mutable state.

```rust
impl<T: PlanTask> Plan<T> {
    pub fn work_groups(&self, prefix: &str) -> Vec<PlanWorkGroup> {
        canonical_work_groups_from_batches(&self.batches, prefix)
    }
}
```

If stored for serialization, regenerate on every mutation:

```rust
impl<T: PlanTask> Plan<T> {
    pub fn reconcile_work_groups(&mut self, prefix: &str) {
        self.work_groups = canonical_work_groups_from_batches(&self.batches, prefix);
    }

    /// Prune tasks and batches, then reconcile work_groups. Returns removed task IDs.
    pub fn prune_tasks(&mut self, keep: impl Fn(&T) -> bool, prefix: &str) -> Vec<String> {
        let removed = /* prune logic */;
        self.reconcile_work_groups(prefix);
        removed
    }
}
```

The key insight: **pruning and work_group regeneration must be a single atomic operation**. They cannot be separate functions that callers must remember to compose.

### S2: Make `GroundedPlan<T>::try_from` require grounding context

**Problem**: `GroundedModelPlan::try_from` accepts `None` for allowed inputs, defeating the purpose of the `Grounded` newtype.

**Solution**: Change the construction API so the grounding context is required:

```rust
pub struct GroundedModelPlan(pub(crate) ModelPlan);

impl GroundedModelPlan {
    pub fn ground(
        mut plan: ModelPlan,
        allowed_staging_models: &BTreeSet<String>,
    ) -> Result<Self, String> {
        ensure_expected_model_paths_model(&mut plan);
        plan.prune_tasks(/* staging filter */, "model");
        let mut errors = strict_model_grounding_errors(&plan, Some(allowed_staging_models));
        let sem = validate_model_plan_semantics(&plan, Some(allowed_staging_models));
        errors.extend(sem.messages());
        if !errors.is_empty() {
            return Err(format!("model_plan_grounding_failed: {}", errors.join(" | ")));
        }
        Ok(Self(plan))
    }
}
```

Remove `TryFrom<ModelPlan>` entirely. The only way to construct a `GroundedModelPlan` is through `ground()` which requires the allowlist. The compiler enforces that callers provide it.

### S3: Unify the plan pipeline into a single generic function

**Problem**: Cleanse and model follow different orderings (prune-then-enrich vs enrich-then-prune) with no compiler enforcement of the correct sequence.

**Solution**: Define the pipeline as a sequence of typed stages:

```rust
pub struct CompiledPlan<T: PlanTask>(Plan<T>);      // skeleton compiled, not yet enriched
pub struct EnrichedPlan<T: PlanTask>(Plan<T>);       // all tasks have specs
pub struct GroundedPlan<T: PlanTask>(Plan<T>);       // pruned, validated, ready to persist

impl<T: PlanTask> CompiledPlan<T> {
    /// Prune ungroundable tasks, then enrich survivors.
    pub async fn enrich_and_ground(
        self,
        allowlist: &BTreeSet<String>,
        enrich_fn: impl AsyncFn(&mut Plan<T>, &[String]) -> Result<(), String>,
        prefix: &str,
    ) -> Result<GroundedPlan<T>, String> {
        let mut plan = self.0;
        plan.prune_tasks(/* filter using allowlist */, prefix);
        let surviving_ids = plan.tasks.iter().map(|t| t.task_id().to_string()).collect();
        enrich_fn(&mut plan, &surviving_ids).await?;
        // validate + return GroundedPlan
    }
}
```

This makes it **impossible** to enrich before pruning — the type system only allows `CompiledPlan → GroundedPlan` through a function that prunes first.

### S4: Remove `save_*_plan_grounded` re-pruning

**Problem**: `save_model_plan_grounded` re-prunes a clone internally, which can re-introduce I1.

**Solution**: `save_*_plan_grounded` should accept `GroundedPlan<T>` (not `Plan<T>` + optional allowlist). Since `GroundedPlan` is already validated, persistence is a simple serialization with no further mutation.

```rust
pub async fn save_grounded_plan<T: PlanTask + Serialize>(
    ctx: &AgentCtx,
    plan: &GroundedPlan<T>,
) -> Result<(), String> {
    let inner = &plan.0;
    let bytes = serde_json::to_vec_pretty(inner).map_err(|e| e.to_string())?;
    ctx.storage()
        .put_bytes(&inner.plan_key, &bytes, "application/json")
        .await
        .map_err(|e| e.to_string())
}
```

### S5: Deduplicate enrichment via `EnrichablePlan` trait (TODO item-86)

**Problem**: Two near-identical ~165-line functions that can drift independently.

**Solution**: Extract a trait:

```rust
trait EnrichablePlan: PlanTask {
    type Spec;
    type EnrichmentItem;
    fn task_id_for_lookup(task: &Self) -> &str;
    fn apply_spec(task: &mut Self, spec: Self::Spec);
    fn is_placeholder(task: &Self) -> bool;
    fn system_prompt() -> String;
    fn schema_spec() -> Result<JsonSchemaSpec, String>;
}
```

Single generic `enrich_tasks<T: EnrichablePlan>()` function handles the chunk→reason→compile→apply→retry→unresolved loop for both plan types.

### S6: Split "missing" from "read failed" in storage/control-state APIs

**Problem**: The current read APIs permit callers to treat storage failure as absence, which makes silent state destruction representable.

**Solution**:

```rust
enum LoadState<T> {
    Missing,
    Loaded(T),
}

fn load_control_state(...) -> Result<LoadState<ExecutionState>, CoreError>;
fn load_thread_log(...) -> Result<LoadState<ThreadLog>, CoreError>;
```

Mutation helpers should only accept `Loaded(T)` and must explicitly branch on `Missing`. A transport/storage failure remains an `Err` and cannot be silently converted into an empty/default value.

### S7: Make "stay in phase" require proof of progress

**Problem**: `StayInPhase` is too weak; it allows no-op loops.

**Solution**:

```rust
enum PhaseExecutorOutcome {
    TransitionCommitted,
    Return(Vec<FlowFrame>),
    StayedWithProgress { detail: String },
    StayedWaiting { reason: String },
    Failed { reason: String },
}
```

This forces every non-transition loopback to declare whether it made progress or is intentionally waiting. Repeated `StayedWaiting` without external change can then be guarded centrally.

### S8: Centralize all run-loop stops through a recorded stop reason

**Problem**: Rejected complete actions and step-limit exits still bypass a single explicit stop-event seam.

**Solution**:
- Introduce a single `record_run_loop_stop(...)` function in core.
- Route all exits from `run_loop.rs` through it.
- Model stop reasons as a typed enum so the compiler forces exhaustive handling.

### S9: Add optimistic concurrency / CAS to thread and control-state writes

**Problem**: Read-modify-write races are still representable.

**Solution**:
- Introduce versioned reads and conditional writes at the storage abstraction.
- Surface write conflicts explicitly so callers must retry/reload rather than silently clobbering.

### S10: Make observed tool execution unwind-safe or explicitly abort-recorded

**Problem**: Panic/unwind paths still permit unmatched tool observability.

**Solution**:
- Either guarantee `ToolEnd` emission via a drop guard/finalizer, or
- make unwind/abort produce a terminal failure marker at the outer boundary before the thread is abandoned.

---

## Priority Order

| Priority | Issue | Fix | Status |
|----------|-------|-----|--------|
| **P0** | I1: Prune doesn't update work_groups | S1: Atomic prune + reconcile | **DONE** |
| **P0** | I4: `GroundedPlan::try_from` skips checks | S2: Require grounding context | **DONE** |
| **P1** | I2: Model enriches before pruning | S3: Reorder pipeline (prune-then-enrich) | **DONE** |
| **P1** | I3: `save_*_grounded` re-prunes | S4: Remove internal re-pruning | **DONE** |
| **P2** | I12: Enrichment duplication | S5: `EnrichablePlan` trait | Open (TODO item-86) |
| **P2** | I5: No work_group reconciliation | Solved by S1 | **DONE** |
| **P0** | I7: Storage reads collapse failure into absence | S6: typed load APIs | Open |
| **P1** | I8: Author phase no-op stay loops | S7: typed stay-with-progress outcomes | Open |
| **P1** | I9: Run-loop stop reasons not fully recorded | S8: central stop recorder | Open |
| **P1** | I10: Read-modify-write races | S9: CAS/versioned writes | Open |
| **P1** | I11: Panic paths can orphan tool observability | S10: unwind-safe observed execution | Open |
