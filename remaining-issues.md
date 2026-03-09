# Remaining Issues — Plan Pipeline Audit

Thread evidence: `3ac65b27`, `3a383d24` (both built SILVER successfully, both died in `model_plan` grounding).

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

### I7: Prior audit items (from previous conversation)

These were identified in the codebase-wide audit and remain open:

| ID | Severity | Issue | File |
|----|----------|-------|------|
| C1 | CRITICAL | Storage read error treated as "empty thread" — silent data destruction | `store_io.rs:66-71` |
| C2 | CRITICAL | Storage read error treated as "no control state" — silent state loss | `control_state.rs:88` |
| H1 | HIGH | `phase_author.rs:906` tight no-op loop when author agent returns with no tool calls | `phase_author.rs` |
| H2 | HIGH | Rejected `Complete` action burns step budget silently | `run_loop.rs:154-163` |
| H3 | HIGH | Step-limit exhaustion leaves no thread log record | `run_loop.rs:188-194` |
| H4 | HIGH | Read-modify-write race in `store_io.rs` and `control_state.rs` | `store_io.rs`, `control_state.rs` |
| H5 | HIGH | Panic in `observed.rs` leaves orphaned ToolStart | `observed.rs` |

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

---

## Priority Order

| Priority | Issue | Fix | Status |
|----------|-------|-----|--------|
| **P0** | I1: Prune doesn't update work_groups | S1: Atomic prune + reconcile | **DONE** |
| **P0** | I4: `GroundedPlan::try_from` skips checks | S2: Require grounding context | **DONE** |
| **P1** | I2: Model enriches before pruning | S3: Reorder pipeline (prune-then-enrich) | **DONE** |
| **P1** | I3: `save_*_grounded` re-prunes | S4: Remove internal re-pruning | **DONE** |
| **P2** | I6: Enrichment duplication | S5: `EnrichablePlan` trait | Open (TODO item-86) |
| **P2** | I5: No work_group reconciliation | Solved by S1 | **DONE** |
| **P3** | I7: Prior audit items (C1, C2, H1-H5) | See previous audit | Open |
