use super::*;
use crate::phase_contract::{commit_phase_decision, PhaseDecision};

impl DataEngineerSuite {
    pub(super) fn collect_targeted_semantic_tasks(
        issues: &[crate::plan::PlanSemanticIssue],
        candidates: &[String],
    ) -> Vec<String> {
        let mut out = Vec::new();
        for issue in issues {
            if let Some(task_id) = issue.task_id.as_ref() {
                let key = task_id.trim();
                if !key.is_empty() && candidates.iter().any(|c| c == key) {
                    out.push(key.to_string());
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    pub(super) async fn approve_plan_draft_and_advance(
        thread_store: &ThreadStore,
        thread_id: &str,
        phase: control_flow::Phase,
        track: crate::track_spec::TrackKind,
        actx: &AgentCtx,
        log_len: usize,
        transition_reason_code: PhaseReasonCode,
        transition_reason_detail: serde_json::Value,
    ) -> Result<bool, String> {
        use crate::phase_plan_lifecycle::{load_plan_for_track, save_plan, TrackPlanDoc};
        use crate::plan_types::TrackPlan;

        let Some(mut doc) = load_plan_for_track(actx, track).await else {
            return Ok(false);
        };
        if doc.status() != crate::plan::PlanStatus::Draft {
            return Ok(false);
        }

        // Pre-compute grounding once so we don't re-run discovery in the semantic check below.
        let staging_grounding = if matches!(&doc, TrackPlanDoc::Model(_)) {
            Some(crate::dataset_truth::discover_staging_models_from_storage(actx).await)
        } else {
            None
        };

        // Defensive grounding at approval time (facts can change; never assume).
        match &mut doc {
            TrackPlanDoc::Cleanse(p) => {
                let mut candidates: Vec<String> = Vec::new();
                for t in p.tasks.iter() {
                    if !t.dataset_id.trim().is_empty() {
                        candidates.push(t.dataset_id.trim().to_string());
                    }
                }
                for b in p.batches.iter() {
                    for ds in b.iter() {
                        if !ds.trim().is_empty() {
                            candidates.push(ds.trim().to_string());
                        }
                    }
                }
                candidates.sort();
                candidates.dedup();
                let wh = crate::ctx_ext::actx_warehouse(actx)
                    .ok_or_else(|| "warehouse provider missing for plan grounding".to_string())?;
                let grounded = crate::dataset_truth::build_grounded_raw_dataset_set(
                    actx, &wh, &candidates,
                ).await;
                crate::plan::prune_cleanse_plan_to_grounded_raw_datasets(p, &grounded.allowed);
            }
            TrackPlanDoc::Model(p) => {
                let stg = staging_grounding.as_ref().expect("pre-computed for model track");
                crate::plan::prune_model_plan_to_grounded_staging_models(p, &stg.allowed_models);
            }
        }

        if doc.is_empty() {
            doc.cancel().map_err(|e| format!("plan cancel failed: {e}"))?;
            save_plan(actx, &doc).await
                .map_err(|e| format!("failed to persist pruned-empty {} plan: {e}", track.as_str()))?;
            commit_phase_decision(
                thread_store, thread_id, Some(phase),
                PhaseDecision::annotation(
                    phase,
                    Some(PhaseReasonCode::PlanPrunedEmpty),
                    Some(crate::phase_reason_detail::to_value(
                        &crate::phase_reason_detail::PlanPrunedEmptyDetail {
                            plan_key: doc.plan_key().to_string(),
                        },
                    )),
                ),
            ).await?;
            return Ok(true);
        }

        // Auto-heal (semantic): reuse the grounding computed above.
        let v = match &mut doc {
            TrackPlanDoc::Cleanse(p) => {
                crate::plan::ensure_cleanse_plan_semantically_valid_or_repaired(p)
            }
            TrackPlanDoc::Model(p) => {
                let stg = staging_grounding.as_ref().expect("pre-computed for model track");
                crate::plan::ensure_model_plan_semantically_valid_or_repaired(
                    p,
                    &stg.allowed_models,
                )
            }
        };
        if !v.ok {
            doc.cancel().map_err(|e| format!("plan cancel failed: {e}"))?;
            save_plan(actx, &doc).await
                .map_err(|e| format!("failed to persist semantically-invalid {} plan: {e}", track.as_str()))?;
            commit_phase_decision(
                thread_store, thread_id, Some(phase),
                PhaseDecision::annotation(
                    phase,
                    Some(PhaseReasonCode::PlanSemanticInvalid),
                    Some(crate::phase_reason_detail::to_value(
                        &crate::phase_reason_detail::PlanSemanticInvalidErrorsDetail {
                            plan_key: doc.plan_key().to_string(),
                            errors: v.errors.clone(),
                        },
                    )),
                ),
            ).await?;
            return Ok(true);
        }

        doc.approve().map_err(|e| format!("plan approval failed: {e}"))?;
        doc.progress_mut().last_applied_step_idx = log_len;
        save_plan(actx, &doc).await
            .map_err(|e| format!("failed to persist approved {} plan: {e}", track.as_str()))?;
        crate::state_manager::mutate_execution_state(
            &thread_store.control_store(), thread_id,
            |es| es.clear_pending_patch_impl(),
        ).await.map_err(|e| format!("failed to clear pending patch impl intent: {e}"))?;

        commit_phase_decision(
            thread_store, thread_id, Some(phase),
            PhaseDecision::forward(
                track.author_phase(),
                Some(transition_reason_code),
                Some(transition_reason_detail),
            ),
        ).await?;
        Ok(true)
    }

    pub(super) async fn authoring_complete_reason_detail(
        thread_store: &ThreadStore,
        thread_id: &str,
        has_proj: bool,
        has_models: bool,
    ) -> serde_json::Value {
        let execution_state = crate::progress_controller::ExecutionState::load(
            &thread_store.control_store(),
            thread_id,
        )
        .await
        .unwrap_or_else(crate::progress_controller::ExecutionState::new);
        let guard = control_flow::derive_guard_state_from_execution_state(&execution_state);
        crate::phase_reason_detail::to_value(
            &crate::phase_reason_detail::AuthoringCompleteReasonDetail {
                invariants: crate::phase_reason_detail::AuthoringCompleteInvariantsDetail {
                    has_dbt_project_yml: has_proj,
                    has_any_models: has_models,
                },
                guard_state: crate::phase_reason_detail::AuthoringCompleteGuardStateDetail {
                    last_validate_failed: guard.last_validate_failed,
                    mutated_since_fail: guard.mutated_since_fail,
                    patched_since_fail: guard.patched_since_fail,
                    mutation_failures_since_validate: guard.mutation_failures_since_validate,
                    probe_required: guard.probe_required,
                    probe_satisfied: guard.probe_satisfied,
                },
            },
        )
    }
    pub(super) async fn has_any_gold_model_sql(actx: &AgentCtx) -> bool {
        let base = actx
            .keyspace()
            .scoped_prefix(actx.scope(), &["dbt"])
            .trim_end_matches('/')
            .to_string();
        let prefixes = [
            format!("{}/models/core/", base),
            format!("{}/models/marts/", base),
        ];
        for pref in prefixes.iter() {
            if let Ok(keys) = actx.storage().list_prefix(pref).await {
                for k in keys {
                    if !k.ends_with(".sql") {
                        continue;
                    }
                    if k.contains("/_versions/") {
                        continue;
                    }
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn strip_meta_line(answer: &str) -> String {
        let mut lines = answer.lines();
        let first = lines.next().unwrap_or("").trim();
        if first.starts_with("META:") {
            lines.collect::<Vec<&str>>().join("\n").trim().to_string()
        } else {
            answer.trim().to_string()
        }
    }

    pub(super) fn build_review_question_with_context(
        question: &str,
        phase: control_flow::Phase,
        execution_state: &crate::progress_controller::ExecutionState,
    ) -> String {
        let mut base = match phase {
            control_flow::Phase::CleanseReview => format!(
                "Review the DBT project after cleanse/staging work. Identify any issues or improvements to apply.\n\nOriginal goal:\n{}",
                question
            ),
            control_flow::Phase::ModelReview => format!(
                "Review the DBT project after modeling (core/gold) work. Identify any issues or improvements to apply.\n\nOriginal goal:\n{}",
                question
            ),
            _ => format!(
                "Final review after publish. Identify any remaining actionable improvements.\n\nOriginal goal:\n{}",
                question
            ),
        };

        let entry_reason_code: Option<PhaseReasonCode> = execution_state.phase.phase_reason_code;
        let entry_reason_detail: serde_json::Value = execution_state
            .phase
            .phase_reason_detail
            .clone()
            .unwrap_or(serde_json::Value::Null);

        let mut prior_review_block: Option<String> = None;
        if matches!(
            entry_reason_code,
            Some(
                PhaseReasonCode::ReviewProceed
                    | PhaseReasonCode::ReviewPatchImpl
            )
        ) {
            let rd = entry_reason_detail.clone();
            let review_phase = rd
                .get("review_phase")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let meta = rd.get("meta").cloned().unwrap_or(serde_json::Value::Null);
            let ans = rd.get("answer").and_then(|v| v.as_str()).unwrap_or("");
            let excerpt = {
                let cleaned = Self::strip_meta_line(ans);
                let max = 700usize;
                if cleaned.len() > max {
                    format!("{}...", &cleaned[..max])
                } else {
                    cleaned
                }
            };

            prior_review_block = Some(format!(
                "Previous review decision:\n- review_phase: {review_phase}\n- meta: {meta}\n- excerpt: {excerpt}",
                review_phase = review_phase,
                meta = meta,
                excerpt = excerpt.replace('\n', " "),
            ));
        }

        let mut ctx_lines: Vec<String> = Vec::new();
        if let Some(prior) = prior_review_block {
            ctx_lines.push(prior);
        }
        if let Some(last_mutation) = execution_state.telemetry.last_mutation_summary.as_ref() {
            ctx_lines.push(format!(
                "Most recent mutation summary (state-derived):\n{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "op": last_mutation.op,
                    "affected_paths": last_mutation.affected_paths,
                    "select_terms": last_mutation.select_terms,
                }))
                .unwrap_or_else(|_| "{}".to_string())
            ));
        }
        if entry_reason_code.is_some() || !entry_reason_detail.is_null() {
            ctx_lines.push(format!(
                "Why we are reviewing now:\n- entry_reason_code: {}\n- entry_reason_detail: {}",
                entry_reason_code.map(|rc| rc.as_str()).unwrap_or("null"),
                entry_reason_detail
            ));
        }

        if !ctx_lines.is_empty() {
            base = format!(
                "Review context (from thread history):\n{}\n\n{}",
                ctx_lines.join("\n\n"),
                base
            );
        }

        base
    }
}
