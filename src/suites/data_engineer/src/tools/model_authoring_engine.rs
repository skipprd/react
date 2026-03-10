use std::collections::{HashMap, HashSet};

use react_core::agent::AgentCtx;
use serde_json::Value;

use crate::{authoring_ir, plan_types::OutputFieldSpec, project_fs, sql_first};

pub(crate) fn emit_trace(ctx: &AgentCtx, line: impl Into<String>) {
    if let Some(tx) = ctx.trace_tx().as_ref() {
        let _ = tx.send(line.into());
    }
}

pub(crate) fn extract_string_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub(crate) fn athena_alias_reuse_hint(
    msg: &str,
    model_rel_path: &str,
    entity_id: &str,
) -> Option<Value> {
    let m = msg.to_ascii_lowercase();
    if !m.contains("select-list alias") {
        return None;
    }
    Some(serde_json::json!({
        "kind": "athena_select_alias_reuse",
        "entity_id": entity_id,
        "model_path": model_rel_path,
        "issue": msg,
        "fix": "Split into CTE + outer select: compute intermediate aliases in an inner CTE/subquery, then reference them only from the outer SELECT."
    }))
}

pub(crate) fn build_provider_prompt_rules(
    wh: Option<&dyn crate::providers::WarehouseProvider>,
) -> String {
    let mut out = String::new();
    if let Some(w) = wh {
        for rule in w.sql_prompt_rules().into_iter() {
            out.push_str("  - ");
            out.push_str(rule);
            out.push('\n');
        }
    }
    out
}

pub(crate) fn dedup_notes(notes: Vec<String>, cap: usize) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for n in notes {
        if seen.insert(n.clone()) {
            out.push(n);
        }
        if out.len() >= cap {
            break;
        }
    }
    out
}

pub(crate) struct CompileAndWriteResult {
    pub key: String,
    pub notes: Vec<String>,
}

/// Compile a SQL-first draft through authoring IR, apply placeholder
/// replacements to produce dbt SQL, run an optional validator, then
/// write to storage via patch.
pub(crate) async fn compile_and_write_model<V>(
    ctx: &AgentCtx,
    draft: &sql_first::SqlFirstDraft,
    plan_output_fields: &[OutputFieldSpec],
    materialize_replacements: &HashMap<String, String>,
    validate_dbt_sql: V,
    existing_sql: &str,
    rel_path: &str,
) -> Result<CompileAndWriteResult, String>
where
    V: FnOnce(&str) -> Result<(), String>,
{
    let intent = authoring_ir::compile_sql_first_draft(
        &draft.sql,
        &draft.notes,
        plan_output_fields,
    )?;

    let dbt_sql = sql_first::apply_placeholders(&intent.sql, materialize_replacements);

    validate_dbt_sql(&dbt_sql)?;

    let base_sha256 = if existing_sql.is_empty() {
        None
    } else {
        Some(react_core::llm_observability::sha256_hex_str(existing_sql))
    };
    let patch_text = project_fs::hunks_only_full_replace_patch(existing_sql, &dbt_sql);
    let outcome = project_fs::apply_patch(
        ctx,
        None,
        rel_path,
        &patch_text,
        base_sha256.as_deref(),
        Some(!existing_sql.is_empty()),
        project_fs::PatchApplyKind::UnifiedDiff,
    )
    .await?;

    ctx.storage()
        .put_bytes(&outcome.key, outcome.content.as_bytes(), "text/sql")
        .await
        .map_err(|e| format!("failed to write model {rel_path}: {e}"))?;

    emit_trace(ctx, format!("saved {}", rel_path));

    Ok(CompileAndWriteResult {
        key: outcome.key,
        notes: intent.notes,
    })
}

/// Run the SQL-first authoring loop: draft via LLM, optionally tweak the draft,
/// validate against the warehouse, and retry up to `max_attempts` times.
pub(crate) struct AuthorLoopConfig {
    pub max_tokens: usize,
    pub max_attempts: usize,
    pub initial_prompt_id: &'static str,
    pub repair_prompt_id: &'static str,
    pub initial_temp: f64,
    pub repair_temp: f64,
}

pub(crate) struct AuthorLoopOutcome {
    pub draft: sql_first::SqlFirstDraft,
    pub remediation_hints: Vec<Value>,
}

pub(crate) async fn sql_first_author_loop<F>(
    ctx: &AgentCtx,
    config: &AuthorLoopConfig,
    sys_prompt_builder: impl Fn() -> String,
    user_value: &Value,
    replacements: &HashMap<String, String>,
    entity_id: &str,
    rel_path: &str,
    mut post_draft_hook: F,
) -> Result<AuthorLoopOutcome, Vec<String>>
where
    F: FnMut(&mut sql_first::SqlFirstDraft) -> Result<(), String>,
{
    let mut last_err = String::new();
    let mut prev_sql: Option<String> = None;
    let mut remediation_hints: Vec<Value> = Vec::new();

    for attempt in 1..=config.max_attempts {
        let mut v = user_value.clone();
        if attempt > 1 {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("attempt".to_string(), serde_json::json!(attempt));
                obj.insert(
                    "previous_sql".to_string(),
                    serde_json::json!(prev_sql.clone().unwrap_or_default()),
                );
                obj.insert("error".to_string(), serde_json::json!(last_err.clone()));
            }
        }

        let sys_msg = sys_prompt_builder();
        let prompt_id = if attempt == 1 {
            config.initial_prompt_id
        } else {
            config.repair_prompt_id
        };
        let temp = if attempt == 1 {
            config.initial_temp
        } else {
            config.repair_temp
        };

        let mut d = match sql_first::llm_draft_sql_json(
            ctx,
            sys_msg,
            v.to_string(),
            prompt_id,
            config.max_tokens as u32,
            temp as f32,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                last_err = e;
                if attempt >= config.max_attempts {
                    return Err(vec![format!("{entity_id}: sql draft failed: {last_err}")]);
                }
                continue;
            }
        };

        if let Err(e) = post_draft_hook(&mut d) {
            last_err = e;
            prev_sql = Some(d.sql.clone());
            if attempt >= config.max_attempts {
                return Err(vec![format!("{entity_id}: sql draft invalid: {last_err}")]);
            }
            continue;
        }

        match sql_first::validate_sql_quick(ctx, &d.sql, replacements).await {
            Ok(()) => {
                return Ok(AuthorLoopOutcome {
                    draft: d,
                    remediation_hints,
                });
            }
            Err(err) => {
                last_err = err.clone();
                prev_sql = Some(d.sql.clone());
                if let Some(h) = athena_alias_reuse_hint(&err, rel_path, entity_id) {
                    remediation_hints.push(h);
                }
            }
        }
        if attempt >= config.max_attempts {
            return Err(vec![format!(
                "{entity_id}: sql validation failed: {last_err}"
            )]);
        }
    }

    Err(vec![format!("{entity_id}: authoring loop exhausted")])
}
