use react_core::agent::AgentCtx;

use crate::data_engineer::plan_grounding::{
    ensure_expected_model_paths_cleanse, ensure_expected_model_paths_model,
    prune_cleanse_plan_to_grounded_raw_datasets, prune_model_plan_to_grounded_staging_models,
};
use crate::data_engineer::plan_types::*;

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

async fn list_plan_keys(ctx: &AgentCtx, suffix: &str) -> Vec<String> {
    let pref = plans_thread_prefix(ctx);
    let mut keys = ctx.storage().list_prefix(&pref).await.unwrap_or_default();
    keys.retain(|k| k.ends_with(suffix));
    keys.sort();
    keys
}

/// Return the newest (lexicographically largest) plan key for this thread, regardless of terminal status.
///
/// Plan keys are timestamp-prefixed, so lexicographic ordering matches recency.
/// This is intentionally different from `load_cleanse_plan`/`load_model_plan`, which prefer the
/// oldest non-terminal plan to match the deterministic pipeline behavior.
pub async fn newest_plan_key_any(ctx: &AgentCtx, suffix: &str) -> Option<String> {
    let keys = list_plan_keys(ctx, suffix).await;
    keys.last().cloned()
}

async fn oldest_active_cleanse_plan_key(ctx: &AgentCtx) -> Option<String> {
    let keys = list_plan_keys(ctx, "_cleanse.json").await;
    for k in keys {
        if let Ok(bytes) = ctx.storage().get_bytes(&k).await {
            if let Ok(p) = serde_json::from_slice::<CleansePlan>(&bytes) {
                if !p.status.is_terminal() {
                    return Some(k);
                }
            }
        }
    }
    None
}

async fn oldest_active_model_plan_key(ctx: &AgentCtx) -> Option<String> {
    let keys = list_plan_keys(ctx, "_model.json").await;
    for k in keys {
        if let Ok(bytes) = ctx.storage().get_bytes(&k).await {
            if let Ok(p) = serde_json::from_slice::<ModelPlan>(&bytes) {
                if !p.status.is_terminal() {
                    return Some(k);
                }
            }
        }
    }
    None
}

pub async fn load_cleanse_plan(ctx: &AgentCtx) -> Option<CleansePlan> {
    let key = oldest_active_cleanse_plan_key(ctx).await?;
    load_cleanse_plan_by_key(ctx, &key).await
}

/// Load the cleanse plan for this thread, preferring the oldest non-terminal plan.
///
/// If there is no active plan (e.g. a restart after we marked it Completed), fall back to the
/// newest plan key so "continue" can rehydrate context and progress deterministically.
pub async fn load_cleanse_plan_any(ctx: &AgentCtx) -> Option<CleansePlan> {
    if let Some(p) = load_cleanse_plan(ctx).await {
        return Some(p);
    }
    let key = newest_plan_key_any(ctx, "_cleanse.json").await?;
    load_cleanse_plan_by_key(ctx, &key).await
}

pub async fn load_cleanse_plan_by_key(ctx: &AgentCtx, key: &str) -> Option<CleansePlan> {
    let bytes = ctx.storage().get_bytes(key).await.ok()?;
    let mut p = serde_json::from_slice::<CleansePlan>(&bytes).ok()?;
    if p.plan_key.trim().is_empty() {
        p.plan_key = key.to_string();
    }
    let changed = ensure_expected_model_paths_cleanse(Some(ctx), &mut p);
    if changed {
        if let Err(e) = save_cleanse_plan(ctx, &p).await {
            tracing::warn!("failed to persist canonicalized cleanse plan paths: {}", e);
        }
    }
    Some(p)
}

pub async fn save_cleanse_plan(ctx: &AgentCtx, plan: &CleansePlan) -> Result<(), String> {
    if plan.plan_key.trim().is_empty() {
        return Err("cleanse plan missing plan_key".to_string());
    }
    let candidate = PersistableCleansePlan::try_from(plan.clone())?.into_inner();
    let bytes = serde_json::to_vec_pretty(&candidate).map_err(|e| e.to_string())?;
    ctx.storage()
        .put_bytes(&candidate.plan_key, &bytes, "application/json")
        .await
        .map_err(|e| e.to_string())
}

pub async fn save_cleanse_plan_grounded(
    ctx: &AgentCtx,
    plan: &CleansePlan,
    allowed_raw: Option<&std::collections::BTreeSet<String>>,
) -> Result<(), String> {
    let mut candidate = plan.clone();
    ensure_expected_model_paths_cleanse(Some(ctx), &mut candidate);
    if let Some(allowed) = allowed_raw {
        prune_cleanse_plan_to_grounded_raw_datasets(&mut candidate, allowed);
    }
    let grounded = GroundedCleansePlan::try_from(candidate)?;
    save_cleanse_plan(ctx, &grounded.0).await
}

pub async fn load_model_plan(ctx: &AgentCtx) -> Option<ModelPlan> {
    let key = oldest_active_model_plan_key(ctx).await?;
    load_model_plan_by_key(ctx, &key).await
}

/// Load the model plan for this thread, preferring the oldest non-terminal plan.
///
/// If there is no active plan, fall back to the newest plan key so "continue" can rehydrate.
pub async fn load_model_plan_any(ctx: &AgentCtx) -> Option<ModelPlan> {
    if let Some(p) = load_model_plan(ctx).await {
        return Some(p);
    }
    let key = newest_plan_key_any(ctx, "_model.json").await?;
    load_model_plan_by_key(ctx, &key).await
}

pub async fn load_model_plan_by_key(ctx: &AgentCtx, key: &str) -> Option<ModelPlan> {
    let bytes = ctx.storage().get_bytes(key).await.ok()?;
    let mut p = serde_json::from_slice::<ModelPlan>(&bytes).ok()?;
    if p.plan_key.trim().is_empty() {
        p.plan_key = key.to_string();
    }
    ensure_expected_model_paths_model(&mut p);
    Some(p)
}

pub async fn save_model_plan(ctx: &AgentCtx, plan: &ModelPlan) -> Result<(), String> {
    if plan.plan_key.trim().is_empty() {
        return Err("model plan missing plan_key".to_string());
    }
    let candidate = PersistableModelPlan::try_from(plan.clone())?.into_inner();
    let bytes = serde_json::to_vec_pretty(&candidate).map_err(|e| e.to_string())?;
    ctx.storage()
        .put_bytes(&candidate.plan_key, &bytes, "application/json")
        .await
        .map_err(|e| e.to_string())
}

pub async fn save_model_plan_grounded(
    ctx: &AgentCtx,
    plan: &ModelPlan,
    allowed_staging_models: Option<&std::collections::BTreeSet<String>>,
) -> Result<(), String> {
    let mut candidate = plan.clone();
    ensure_expected_model_paths_model(&mut candidate);
    if let Some(allowed) = allowed_staging_models {
        prune_model_plan_to_grounded_staging_models(&mut candidate, allowed);
    }
    let grounded = GroundedModelPlan::try_from(candidate)?;
    save_model_plan(ctx, &grounded.0).await
}
