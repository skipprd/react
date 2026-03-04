use super::*;
use crate::data_engineer::phase_contract::{commit_phase_decision, PhaseDecision};

impl DataEngineerSuite {
    pub(super) fn collect_targeted_semantic_tasks(
        issues: &[crate::data_engineer::plan::PlanSemanticIssue],
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

    pub(super) async fn approve_cleanse_plan_draft_and_advance(
        thread_store: &ThreadStore,
        thread_id: &str,
        phase: control_flow::Phase,
        actx: &AgentCtx,
        log_len: usize,
        transition_reason_code: PhaseReasonCode,
        transition_reason_detail: serde_json::Value,
    ) -> Result<bool, String> {
        let Some(mut p) = crate::data_engineer::plan::load_cleanse_plan(actx).await else {
            return Ok(false);
        };
        if p.status != crate::data_engineer::plan::PlanStatus::Draft {
            return Ok(false);
        }

        // Defensive grounding at approval time (facts can change; never assume).
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
        let grounded = crate::data_engineer::dataset_truth::build_grounded_raw_dataset_set(
            actx,
            &actx.warehouse,
            &candidates,
        )
        .await;
        crate::data_engineer::plan::prune_cleanse_plan_to_grounded_raw_datasets(
            &mut p,
            &grounded.allowed,
        );
        if p.tasks.is_empty() || p.batches.is_empty() {
            p.status = crate::data_engineer::plan::PlanStatus::Cancelled;
            crate::data_engineer::plan::save_cleanse_plan(actx, &p)
                .await
                .map_err(|e| format!("failed to persist pruned-empty cleanse plan: {e}"))?;
            // Stay in plan phase; the next iteration will generate a new plan.
            commit_phase_decision(
                thread_store,
                thread_id,
                Some(phase),
                PhaseDecision::annotation(
                    phase,
                    Some(PhaseReasonCode::PlanPrunedEmpty),
                    Some(crate::data_engineer::phase_reason_detail::to_value(
                        &crate::data_engineer::phase_reason_detail::PlanPrunedEmptyDetail {
                            plan_key: p.plan_key.clone(),
                        },
                    )),
                ),
            )
            .await?;
            return Ok(true);
        }

        // Auto-heal (semantic): ensure the approved plan is executable (or cancel so we can replan).
        let v = crate::data_engineer::plan::ensure_cleanse_plan_semantically_valid_or_repaired(
            actx, &mut p,
        )
        .await?;
        if !v.ok {
            p.status = crate::data_engineer::plan::PlanStatus::Cancelled;
            crate::data_engineer::plan::save_cleanse_plan(actx, &p)
                .await
                .map_err(|e| format!("failed to persist semantically-invalid cleanse plan: {e}"))?;
            commit_phase_decision(
                thread_store,
                thread_id,
                Some(phase),
                PhaseDecision::annotation(
                    phase,
                    Some(PhaseReasonCode::PlanSemanticInvalid),
                    Some(crate::data_engineer::phase_reason_detail::to_value(
                        &crate::data_engineer::phase_reason_detail::PlanSemanticInvalidErrorsDetail {
                            plan_key: p.plan_key.clone(),
                            errors: v.errors.clone(),
                        },
                    )),
                ),
            )
            .await?;
            return Ok(true);
        }

        p.status = crate::data_engineer::plan::PlanStatus::Approved;
        // Scope progress to *this* plan instance so old tool calls can't auto-complete a newly approved plan.
        p.progress.last_applied_step_idx = log_len;
        crate::data_engineer::plan::save_cleanse_plan(actx, &p)
            .await
            .map_err(|e| format!("failed to persist approved cleanse plan: {e}"))?;
        crate::data_engineer::state_manager::mutate_execution_state(
            thread_store,
            thread_id,
            |es| es.clear_pending_loopback_intent(),
        )
        .await
        .map_err(|e| format!("failed to clear pending loopback intent: {e}"))?;

        commit_phase_decision(
            thread_store,
            thread_id,
            Some(phase),
            PhaseDecision::forward(
                control_flow::Phase::CleanseAuthor,
                Some(transition_reason_code),
                Some(transition_reason_detail),
            ),
        )
        .await?;
        Ok(true)
    }

    pub(super) async fn approve_model_plan_draft_and_advance(
        thread_store: &ThreadStore,
        thread_id: &str,
        phase: control_flow::Phase,
        actx: &AgentCtx,
        log_len: usize,
        transition_reason_code: PhaseReasonCode,
        transition_reason_detail: serde_json::Value,
    ) -> Result<bool, String> {
        let Some(mut p) = crate::data_engineer::plan::load_model_plan(actx).await else {
            return Ok(false);
        };
        if p.status != crate::data_engineer::plan::PlanStatus::Draft {
            return Ok(false);
        }

        // Defensive grounding at approval time: gold must rely only on existing staging models.
        let stg =
            crate::data_engineer::dataset_truth::discover_staging_models_from_storage(actx).await;
        crate::data_engineer::plan::prune_model_plan_to_grounded_staging_models(
            &mut p,
            &stg.allowed_models,
        );
        if p.tasks.is_empty() || p.batches.is_empty() {
            p.status = crate::data_engineer::plan::PlanStatus::Cancelled;
            crate::data_engineer::plan::save_model_plan(actx, &p)
                .await
                .map_err(|e| format!("failed to persist pruned-empty model plan: {e}"))?;
            // Stay in plan phase; the next iteration will generate a new plan.
            commit_phase_decision(
                thread_store,
                thread_id,
                Some(phase),
                PhaseDecision::annotation(
                    phase,
                    Some(PhaseReasonCode::PlanPrunedEmpty),
                    Some(crate::data_engineer::phase_reason_detail::to_value(
                        &crate::data_engineer::phase_reason_detail::PlanPrunedEmptyDetail {
                            plan_key: p.plan_key.clone(),
                        },
                    )),
                ),
            )
            .await?;
            return Ok(true);
        }

        // Auto-heal (semantic): ensure the approved plan is executable (or cancel so we can replan).
        let v = crate::data_engineer::plan::ensure_model_plan_semantically_valid_or_repaired(
            actx,
            &mut p,
            &stg.allowed_models,
        )
        .await?;
        if !v.ok {
            p.status = crate::data_engineer::plan::PlanStatus::Cancelled;
            crate::data_engineer::plan::save_model_plan(actx, &p)
                .await
                .map_err(|e| format!("failed to persist semantically-invalid model plan: {e}"))?;
            commit_phase_decision(
                thread_store,
                thread_id,
                Some(phase),
                PhaseDecision::annotation(
                    phase,
                    Some(PhaseReasonCode::PlanSemanticInvalid),
                    Some(crate::data_engineer::phase_reason_detail::to_value(
                        &crate::data_engineer::phase_reason_detail::PlanSemanticInvalidErrorsDetail {
                            plan_key: p.plan_key.clone(),
                            errors: v.errors.clone(),
                        },
                    )),
                ),
            )
            .await?;
            return Ok(true);
        }

        p.status = crate::data_engineer::plan::PlanStatus::Approved;
        p.progress.last_applied_step_idx = log_len;
        crate::data_engineer::plan::save_model_plan(actx, &p)
            .await
            .map_err(|e| format!("failed to persist approved model plan: {e}"))?;
        crate::data_engineer::state_manager::mutate_execution_state(
            thread_store,
            thread_id,
            |es| es.clear_pending_loopback_intent(),
        )
        .await
        .map_err(|e| format!("failed to clear pending loopback intent: {e}"))?;

        commit_phase_decision(
            thread_store,
            thread_id,
            Some(phase),
            PhaseDecision::forward(
                control_flow::Phase::ModelAuthor,
                Some(transition_reason_code),
                Some(transition_reason_detail),
            ),
        )
        .await?;
        Ok(true)
    }

    pub(super) async fn authoring_complete_reason_detail(
        thread_store: &ThreadStore,
        thread_id: &str,
        has_proj: bool,
        has_models: bool,
    ) -> serde_json::Value {
        let execution_state = crate::data_engineer::progress_controller::ExecutionState::load(
            thread_store,
            thread_id,
        )
        .await
        .unwrap_or_else(crate::data_engineer::progress_controller::ExecutionState::new);
        let guard = control_flow::derive_guard_state_from_execution_state(&execution_state);
        crate::data_engineer::phase_reason_detail::to_value(
            &crate::data_engineer::phase_reason_detail::AuthoringCompleteReasonDetail {
                invariants: crate::data_engineer::phase_reason_detail::AuthoringCompleteInvariantsDetail {
                    has_dbt_project_yml: has_proj,
                    has_any_models: has_models,
                },
                guard_state: crate::data_engineer::phase_reason_detail::AuthoringCompleteGuardStateDetail {
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
            .keyspace
            .dbt_prefix(&actx.scope)
            .trim_end_matches('/')
            .to_string();
        let prefixes = [
            format!("{}/models/core/", base),
            format!("{}/models/marts/", base),
        ];
        for pref in prefixes.iter() {
            if let Ok(keys) = actx.storage.list_prefix(pref).await {
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
        execution_state: &crate::data_engineer::progress_controller::ExecutionState,
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
