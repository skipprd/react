use react_core::agent::AgentCtx;

use crate::plan_grounding::{
    ensure_expected_model_paths_cleanse, ensure_expected_model_paths_model,
    prune_cleanse_plan_to_grounded_raw_datasets,
};
use crate::plan_types::*;

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("plan storage failed: {0}")]
    StorageFailed(String),
    #[error("plan deserialization failed for '{key}': {detail}")]
    DeserializeFailed { key: String, detail: String },
    #[error("plan save failed: {0}")]
    SaveFailed(String),
    #[error("plan key missing on plan document")]
    MissingPlanKey,
    #[error("plan grounding failed: {0}")]
    GroundingFailed(String),
}

fn thread_dir(ctx: &AgentCtx) -> String {
    ctx.thread_id()
        .as_deref()
        .unwrap_or("no_thread")
        .trim()
        .to_string()
}

fn utc_timestamp_compact() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

fn plans_thread_prefix(ctx: &AgentCtx) -> String {
    let root = ctx
        .keyspace()
        .threads_prefix(ctx.scope())
        .trim_end_matches("/threads")
        .trim_end_matches('/')
        .to_string();
    let tid = thread_dir(ctx);
    format!("{}/plans/{}/", root, tid)
}

pub fn new_cleanse_plan_key(ctx: &AgentCtx) -> String {
    let pref = plans_thread_prefix(ctx);
    format!("{}{}_cleanse.json", pref, utc_timestamp_compact())
}

pub fn new_model_plan_key(ctx: &AgentCtx) -> String {
    let pref = plans_thread_prefix(ctx);
    format!("{}{}_model.json", pref, utc_timestamp_compact())
}

async fn list_plan_keys(ctx: &AgentCtx, suffix: &str) -> Result<Vec<String>, PlanError> {
    let pref = plans_thread_prefix(ctx);
    let mut keys = ctx
        .storage()
        .list_prefix(&pref)
        .await
        .map_err(|e| PlanError::StorageFailed(e.to_string()))?;
    keys.retain(|k| k.ends_with(suffix));
    keys.sort();
    Ok(keys)
}

/// Load the cleanse plan for this thread.
///
/// Prefers the oldest non-terminal plan. If none exists (e.g. plan was marked Completed),
/// falls back to the newest plan key so progress and context are always available.
pub async fn load_cleanse_plan(ctx: &AgentCtx) -> Result<Option<CleansePlan>, PlanError> {
    let keys = list_plan_keys(ctx, "_cleanse.json").await?;
    for k in &keys {
        let bytes = match ctx.storage().get_bytes(k).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("skipping plan key {k}: storage read failed: {e}");
                continue;
            }
        };
        match serde_json::from_slice::<CleansePlan>(&bytes) {
            Ok(p) if !p.status.is_terminal() => return load_cleanse_plan_by_key(ctx, k).await,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("skipping plan key {k}: deserialization failed: {e}");
            }
        }
    }
    let Some(key) = keys.last() else {
        return Ok(None);
    };
    load_cleanse_plan_by_key(ctx, key).await
}

pub async fn load_cleanse_plan_by_key(
    ctx: &AgentCtx,
    key: &str,
) -> Result<Option<CleansePlan>, PlanError> {
    let bytes = match ctx.storage().get_bytes(key).await {
        Ok(b) => b,
        Err(e) => {
            return Err(PlanError::StorageFailed(format!(
                "failed to read plan key '{key}': {e}"
            )));
        }
    };
    let mut p = serde_json::from_slice::<CleansePlan>(&bytes).map_err(|e| {
        PlanError::DeserializeFailed {
            key: key.to_string(),
            detail: e.to_string(),
        }
    })?;
    if p.plan_key.trim().is_empty() {
        p.plan_key = key.to_string();
    }
    let changed = ensure_expected_model_paths_cleanse(Some(ctx), &mut p);
    if changed {
        if let Err(e) = save_cleanse_plan(ctx, &p).await {
            tracing::warn!("failed to persist canonicalized cleanse plan paths: {e}");
        }
    }
    Ok(Some(p))
}

/// Persist a cleanse plan without re-validating grounding. Used for mid-execution
/// mutations on plans that were already grounded at creation time.
pub async fn save_cleanse_plan(ctx: &AgentCtx, plan: &CleansePlan) -> Result<(), PlanError> {
    if plan.plan_key.trim().is_empty() {
        return Err(PlanError::MissingPlanKey);
    }
    let bytes =
        serde_json::to_vec_pretty(plan).map_err(|e| PlanError::SaveFailed(e.to_string()))?;
    ctx.storage()
        .put_bytes(&plan.plan_key, &bytes, "application/json")
        .await
        .map_err(|e| PlanError::SaveFailed(e.to_string()))
}

/// Validate grounding and persist. Returns the `GroundedCleansePlan` proof so
/// callers can carry forward the evidence of successful grounding.
pub async fn save_cleanse_plan_grounded(
    ctx: &AgentCtx,
    plan: &CleansePlan,
    allowed_raw: &std::collections::BTreeSet<String>,
) -> Result<GroundedCleansePlan, PlanError> {
    let mut candidate = plan.clone();
    ensure_expected_model_paths_cleanse(Some(ctx), &mut candidate);
    prune_cleanse_plan_to_grounded_raw_datasets(&mut candidate, allowed_raw);
    let grounded = GroundedCleansePlan::try_from(candidate).map_err(PlanError::GroundingFailed)?;
    save_cleanse_plan(ctx, &grounded.0).await?;
    Ok(grounded)
}

/// Load the model plan for this thread.
///
/// Prefers the oldest non-terminal plan. If none exists, falls back to the newest plan key.
pub async fn load_model_plan(ctx: &AgentCtx) -> Result<Option<ModelPlan>, PlanError> {
    let keys = list_plan_keys(ctx, "_model.json").await?;
    for k in &keys {
        let bytes = match ctx.storage().get_bytes(k).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("skipping plan key {k}: storage read failed: {e}");
                continue;
            }
        };
        match serde_json::from_slice::<ModelPlan>(&bytes) {
            Ok(p) if !p.status.is_terminal() => return load_model_plan_by_key(ctx, k).await,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("skipping plan key {k}: deserialization failed: {e}");
            }
        }
    }
    let Some(key) = keys.last() else {
        return Ok(None);
    };
    load_model_plan_by_key(ctx, key).await
}

pub async fn load_model_plan_by_key(
    ctx: &AgentCtx,
    key: &str,
) -> Result<Option<ModelPlan>, PlanError> {
    let bytes = match ctx.storage().get_bytes(key).await {
        Ok(b) => b,
        Err(e) => {
            return Err(PlanError::StorageFailed(format!(
                "failed to read plan key '{key}': {e}"
            )));
        }
    };
    let mut p =
        serde_json::from_slice::<ModelPlan>(&bytes).map_err(|e| PlanError::DeserializeFailed {
            key: key.to_string(),
            detail: e.to_string(),
        })?;
    if p.plan_key.trim().is_empty() {
        p.plan_key = key.to_string();
    }
    ensure_expected_model_paths_model(&mut p);
    Ok(Some(p))
}

/// Persist a model plan without re-validating grounding. Used for mid-execution
/// mutations (checklist updates, batch progress, status changes) on plans that
/// were already grounded at creation time via `save_model_plan_grounded`.
pub async fn save_model_plan(ctx: &AgentCtx, plan: &ModelPlan) -> Result<(), PlanError> {
    if plan.plan_key.trim().is_empty() {
        return Err(PlanError::MissingPlanKey);
    }
    let bytes =
        serde_json::to_vec_pretty(plan).map_err(|e| PlanError::SaveFailed(e.to_string()))?;
    ctx.storage()
        .put_bytes(&plan.plan_key, &bytes, "application/json")
        .await
        .map_err(|e| PlanError::SaveFailed(e.to_string()))
}

/// Validate grounding against the staging model allowlist and persist.
/// Returns the `GroundedModelPlan` proof so callers can carry forward the
/// evidence of successful grounding.
pub async fn save_model_plan_grounded(
    ctx: &AgentCtx,
    plan: &ModelPlan,
    allowed_staging_models: &std::collections::BTreeSet<String>,
) -> Result<GroundedModelPlan, PlanError> {
    let candidate = plan.clone();
    let grounded = GroundedModelPlan::ground(candidate, allowed_staging_models)
        .map_err(PlanError::GroundingFailed)?;
    save_model_plan(ctx, &grounded.0).await?;
    Ok(grounded)
}
