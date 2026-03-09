use super::*;
use crate::control_flow::Phase;

fn append_validate_fail_facts(
    snapshot: &mut serde_json::Value,
    facts_bundle: &impl serde::Serialize,
    max_entries: usize,
) {
    if snapshot.is_null() {
        *snapshot = serde_json::json!({});
    }
    if let Some(obj) = snapshot.as_object_mut() {
        let arr = obj
            .entry("validate_fail_facts")
            .or_insert_with(|| serde_json::Value::Array(vec![]));
        if let Some(a) = arr.as_array_mut() {
            a.push(
                serde_json::to_value(facts_bundle)
                    .unwrap_or(serde_json::Value::Null),
            );
            while a.len() > max_entries {
                a.remove(0);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValidatePassTransition {
    ToAuthoring,
    ToReview,
}

impl DataEngineerSuite {
    fn decide_validate_pass_transition(
        completion_snapshot: Option<&crate::plan::PlanCompletionSnapshot>,
    ) -> ValidatePassTransition {
        match completion_snapshot
            .map(|snap| snap.completion_state())
            .unwrap_or(crate::plan::PlanCompletionState::Complete)
        {
            crate::plan::PlanCompletionState::Incomplete => {
                ValidatePassTransition::ToAuthoring
            }
            crate::plan::PlanCompletionState::Complete => {
                ValidatePassTransition::ToReview
            }
        }
    }

    async fn reduce_validate_pass_plan_state(
        actx: &AgentCtx,
        phase: Phase,
    ) -> Result<
        (
            Option<crate::plan::PlanCompletionSnapshot>,
            Option<String>,
        ),
        String,
    > {
        let mut completion_snapshot: Option<
            crate::plan::PlanCompletionSnapshot,
        > = None;
        let mut active_plan_key: Option<String> = None;
        if phase == Phase::CleanseValidate {
            if let Some(mut p) = crate::plan::load_cleanse_plan_any(actx).await
            {
                crate::plan::apply_cleanse_progress_event(
                    &mut p,
                    crate::plan::PlanProgressEvent::CleanseValidateDone,
                );
                let snap = crate::plan::snapshot_cleanse_completion(&p);
                active_plan_key = Some(p.plan_key.clone());
                if snap.all_done {
                    p.status = crate::plan::PlanStatus::Completed;
                } else {
                    p.status = crate::plan::PlanStatus::Approved;
                }
                completion_snapshot = Some(snap);
                crate::plan::save_cleanse_plan(actx, &p).await.map_err(|e| {
                    format!("failed to persist cleanse plan validate-pass state: {e}")
                })?;
            }
        } else if let Some(mut p) = crate::plan::load_model_plan_any(actx).await {
            crate::plan::apply_model_progress_event(
                &mut p,
                crate::plan::PlanProgressEvent::ModelValidateDone,
            );
            let snap = crate::plan::snapshot_model_completion(&p);
            active_plan_key = Some(p.plan_key.clone());
            if snap.all_done {
                p.status = crate::plan::PlanStatus::Completed;
            } else {
                p.status = crate::plan::PlanStatus::Approved;
            }
            completion_snapshot = Some(snap);
            crate::plan::save_model_plan(actx, &p).await.map_err(|e| {
                format!("failed to persist model plan validate-pass state: {e}")
            })?;
        }
        Ok((completion_snapshot, active_plan_key))
    }

    async fn commit_validate_pass_transition(
        thread_store: &ThreadStore,
        thread_id: &str,
        phase: Phase,
        thread_state_step_count: usize,
        completion_snapshot: Option<crate::plan::PlanCompletionSnapshot>,
        active_plan_key: Option<String>,
        dbt_validate_observation: serde_json::Value,
        signal: &str,
    ) -> Result<(), String> {
        if Self::decide_validate_pass_transition(completion_snapshot.as_ref())
            == ValidatePassTransition::ToAuthoring
        {
            let reason = format!(
                "{}: dbt_validate succeeded but plan checklist work is still pending; returning to authoring",
                signal
            );
            let detail = crate::phase_reason_detail::to_value(
                &crate::phase_reason_detail::ValidatePassToAuthoringDetail {
                    signal: signal.to_string(),
                    plan_key: active_plan_key,
                    pending_count: completion_snapshot
                        .as_ref()
                        .map(|snap| snap.pending_count)
                        .unwrap_or(0),
                    pending_refs: completion_snapshot
                        .as_ref()
                        .map(|snap| snap.pending_refs.clone())
                        .map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Null))
                        .unwrap_or(serde_json::Value::Array(vec![])),
                    dbt_validate_observation,
                    next_action: crate::phase_reason_detail::ValidateToAuthoringNextAction::ResumeAuthoringForRemainingPlanWork,
                    audit_acceptance: Self::churn_audit_acceptance_criteria(),
                },
            );
            crate::retry_budget::guard_block_loopback_to_author(
                thread_store,
                thread_id,
                phase,
                GuardBlockKind::AuthoringCompletion,
                reason,
                PhaseReasonCode::ValidatePassToAuthoring,
                Some(detail),
            )
            .await?;
            return Ok(());
        }

        let to_phase = if phase == Phase::CleanseValidate {
            Phase::CleanseReview
        } else {
            Phase::ModelReview
        };
        let trigger_step_idx = thread_state_step_count.saturating_sub(1);
        crate::phase_contract::commit_phase_decision(
            thread_store,
            thread_id,
            Some(phase),
            crate::phase_contract::PhaseDecision::forward(
                to_phase,
                Some(PhaseReasonCode::ValidatePassToReview),
                Some(crate::phase_reason_detail::to_value(
                    &crate::phase_reason_detail::ValidatePassToReviewDetail {
                        dbt_validate_observation,
                        dbt_validate_step_idx: trigger_step_idx,
                    },
                )),
            ),
        )
        .await?;
        Ok(())
    }

    pub(super) async fn execute_validate_phase(
        thread_store: &ThreadStore,
        thread_id: &str,
        phase: crate::control_flow::Phase,
        _question: &str,
        sctx: &SuiteCtx,
        _execution_state: &crate::progress_controller::ExecutionState,
        _guard: &crate::control_flow::DerivedGuardState,
        thread_state_step_count: usize,
    ) -> Result<PhaseExecutorOutcome, String> {

let actx = Self::agent_tool_ctx(thread_id, sctx);
{
    let mut es =
        crate::progress_controller::ExecutionState::load(
            &thread_store.control_store(),
            thread_id,
        )
        .await
        .unwrap_or_else(
            crate::progress_controller::ExecutionState::new,
        );
    let tier = if phase == Phase::CleanseValidate {
        crate::progress_controller::ExecutionTier::Cleanse
    } else {
        crate::progress_controller::ExecutionTier::Model
    };
    es.enter_validate_mode(tier);
    es.save(&thread_store.control_store(), thread_id).await.map_err(|e| {
        format!(
            "failed to persist execution state before validate: {e}"
        )
    })?;
}
// Pre-validate normalization: dedupe/merge repeated model+test definitions to
// avoid deterministic compile loops before dbt_validate.
if let Err(e) =
    crate::schema_policy::normalize_schema_artifacts_for_validate(
        &actx,
    )
    .await
{
    use crate::retry_budget::SubjectiveRetryOutcome;
    match Self::check_subjective_retry_budget(
        &thread_store,
        thread_id,
        crate::progress_controller::SubjectiveRetryKind::ValidatePrecheckFailed,
    )
    .await?
    {
        SubjectiveRetryOutcome::Exhausted(tries) => {
            let violation = crate::progress_controller::PlanViolation::new(
                phase,
                None,
                format!("Pre-validation normalization failed {tries} times: {e}"),
            );
            crate::phase_contract::commit_plan_revision_loopback(
                thread_store, thread_id, phase, vec![violation],
                crate::progress_controller::PlanRevisionStrategy::Rewrite,
            ).await?;
            return Ok(PhaseExecutorOutcome::TransitionCommitted);
        }
        SubjectiveRetryOutcome::WithinBudget(_) => {}
    }
    return crate::retry_budget::guard_block_loopback_to_author(
        &thread_store,
        thread_id,
        phase,
        GuardBlockKind::PrecheckFailed,
        format!("Pre-validation normalization failed; fix DBT YAML artifacts before re-validating.\n\n{e}"),
        PhaseReasonCode::PrecheckFailed,
        Some(serde_json::json!({ "error": e })),
    )
    .await;
}

// Cheap structural prechecks: fail fast on malformed/duplicated schema artifacts
// instead of burning a full dbt_validate cycle.
if let Err(e) =
    crate::schema_policy::prevalidate_dbt_schema_artifacts(&actx)
        .await
{
    use crate::retry_budget::SubjectiveRetryOutcome;
    match Self::check_subjective_retry_budget(
        &thread_store,
        thread_id,
        crate::progress_controller::SubjectiveRetryKind::ValidatePrecheckFailed,
    )
    .await?
    {
        SubjectiveRetryOutcome::Exhausted(tries) => {
            let violation = crate::progress_controller::PlanViolation::new(
                phase,
                None,
                format!("Pre-validation structural check failed {tries} times: {e}"),
            );
            crate::phase_contract::commit_plan_revision_loopback(
                thread_store, thread_id, phase, vec![violation],
                crate::progress_controller::PlanRevisionStrategy::Rewrite,
            ).await?;
            return Ok(PhaseExecutorOutcome::TransitionCommitted);
        }
        SubjectiveRetryOutcome::WithinBudget(_) => {}
    }
    return crate::retry_budget::guard_block_loopback_to_author(
        &thread_store,
        thread_id,
        phase,
        GuardBlockKind::PrecheckFailed,
        format!("Pre-validation failed; fix DBT YAML artifacts before re-validating.\n\n{e}"),
        PhaseReasonCode::PrecheckFailed,
        Some(serde_json::json!({ "error": e })),
    )
    .await;
}

// Deterministic full validate (NO repair loop / no mutation).
let obs: crate::controller_event::ValidateObservationContract;
let args_full = serde_json::json!({"build": true});
let tool_id_full = uuid::Uuid::new_v4().to_string();
let mut validate_ctx = react_core::session::ExecutionContext::default();
validate_ctx.set(
    "phase",
    serde_json::Value::String(phase.as_str().to_string()),
);
validate_ctx.set(
    "tier",
    serde_json::Value::String(
        if phase == Phase::CleanseValidate {
            "cleanse"
        } else {
            "model"
        }
        .to_string(),
    ),
);
validate_ctx.set(
    "validate_mode",
    serde_json::Value::String("deterministic_full_build".to_string()),
);
validate_ctx.set(
    "build",
    serde_json::Value::Bool(true),
);
let start_logged = match thread_store
    .append_step(
        thread_id,
        react_core::session::ThreadStep::ToolStart {
            tool_id: tool_id_full.clone(),
            name: "dbt_validate".to_string(),
            clean_name: "Validate DBT".to_string(),
            args: args_full.clone(),
            status: ToolStepStatus::Running,
            payload: None,
            ctx: Some(validate_ctx.clone()),
            ts: chrono::Utc::now().to_rfc3339(),
            agent: "agent".to_string(),
        },
    )
    .await
{
    Ok(_) => true,
    Err(e) => {
        tracing::warn!(
            "data_engineer: failed to append dbt_validate tool_start (thread_id={} phase={}): {}",
            thread_id,
            phase.as_str(),
            e
        );
        false
    }
};
obs = control_flow::DeterministicDbtValidateOnce::run(
    &actx, true, false, None,
)
.await?;
let obs_norm = react_core::session::ToolObservation::normalize(
    obs.observation.clone(),
);
if let Err(e) = thread_store
    .append_step(
        thread_id,
        react_core::session::ThreadStep::ToolEnd {
            tool_id: tool_id_full,
            name: "dbt_validate".to_string(),
            clean_name: "Validate DBT".to_string(),
            args: args_full,
            status: if obs_norm.ok {
                ToolStepStatus::Ok
            } else {
                ToolStepStatus::Failed
            },
            payload: None,
            ctx: Some(validate_ctx),
            observation: obs_norm,
            ts: chrono::Utc::now().to_rfc3339(),
            agent: "agent".to_string(),
        },
    )
    .await
{
    tracing::warn!(
        "data_engineer: failed to append dbt_validate tool_end (thread_id={} phase={}): {}",
        thread_id,
        phase.as_str(),
        e
    );
    if start_logged {
        // Alertable observability gap: we emitted ToolStart but failed to persist ToolEnd.
        // This stable marker is intended for log-based counters/alerts.
        tracing::warn!(
            "data_engineer_observability_gap kind=unmatched_tool_start tool=dbt_validate thread_id={} phase={}",
            thread_id,
            phase.as_str()
        );
    }
}

let validate_event =
    crate::controller_event::validate_event_from_contract(&obs);
if matches!(
    validate_event,
    crate::controller_event::ControllerEvent::ValidatePassed
) {
    // Update canonical execution state (hard-cutover: primary decision source).
    {
        let tier = if phase == Phase::CleanseValidate {
            crate::progress_controller::ExecutionTier::Cleanse
        } else {
            crate::progress_controller::ExecutionTier::Model
        };
        crate::state_manager::apply_execution_event(
            &thread_store.control_store(),
            thread_id,
            crate::progress_controller::DataEngineerEvent::ValidatePassed {
                tier,
            },
        )
        .await
        .map_err(|e| {
            format!("failed to persist execution state after validate pass: {e}")
        })?;
    }

    let (completion_snapshot, active_plan_key) =
        Self::reduce_validate_pass_plan_state(&actx, phase).await?;

    Self::commit_validate_pass_transition(
        &thread_store,
        thread_id,
        phase,
        thread_state_step_count,
        completion_snapshot,
        active_plan_key,
        obs.observation.clone(),
        "ValidatePassPlanIncomplete",
    )
    .await?;
    return Ok(PhaseExecutorOutcome::TransitionCommitted);
}

// Warehouse config failures require user action.
let (
    failure_class,
    failure_signature,
    brief,
    failing_targets,
    compile_ok,
    run_ok,
) = match validate_event {
    crate::controller_event::ControllerEvent::ValidateFailed {
        class,
        signature,
        brief,
        failing_targets,
        compile_ok,
        run_ok,
    } => (class, signature, brief, failing_targets, compile_ok, run_ok),
    crate::controller_event::ControllerEvent::ValidateContractError {
        reason,
        brief,
    } => {
        return Err(format!(
            "validate_outcome_v2_contract_error: {} ({})",
            reason, brief
        ));
    }
    crate::controller_event::ControllerEvent::ValidatePassed => {
        // Covered by the success branch above.
        unreachable!("validate pass should have continued above")
    }
};
if matches!(
    failure_class,
    crate::failure_kind::FailureKind::WarehouseConfig
) {
    return Err(format!(
        "dbt_validate failed due to a warehouse/aws configuration issue: {}",
        brief
    ));
}
let errs: Vec<String> = obs
    .observation
    .get("errors")
    .and_then(|v| serde_json::from_value(v.clone()).ok())
    .unwrap_or_default();

// Update canonical execution state from this validate failure (hard-cutover: primary decision source).
{
    let tier = if phase == Phase::CleanseValidate {
        crate::progress_controller::ExecutionTier::Cleanse
    } else {
        crate::progress_controller::ExecutionTier::Model
    };
    let failing_models: Vec<
        crate::progress_controller::FailedModelRef,
    > = failing_targets
        .iter()
        .map(|t| crate::progress_controller::FailedModelRef {
            name: t.node_id.clone(),
            file: t.target_path.as_str().to_string(),
            ..Default::default()
        })
        .collect();
    let failure_class_state = failure_class;
    let backlog = crate::progress_controller::repair_backlog_from_failed_models(
        failure_class_state,
        &failing_models,
    );
    let repair_intent = crate::progress_controller::repair_intent_from_backlog(
        failure_class_state,
        backlog,
    );
    crate::state_manager::apply_execution_event(
        &thread_store.control_store(),
        thread_id,
        crate::progress_controller::DataEngineerEvent::ValidateFailed {
            tier,
            failure_class: failure_class_state,
            failure_signature: crate::progress_controller::FailureSignature {
                class: failure_class_state,
                node_id: Some(failure_signature.node_id.clone()),
                canonical_path: Some(failure_signature.target_path.clone()),
                error_code: Some(failure_signature.error_code.clone()),
            },
            repair_intent,
            brief: Some(brief.clone()),
            compile_ok: Some(compile_ok),
            run_ok: Some(run_ok),
        },
    )
    .await
    .map_err(|e| {
        format!("failed to persist execution state after validate failure: {e}")
    })?;
}

// Escalate to plan revision if repeated failures indicate the plan itself is broken.
// replan_backtracks reflects how many validate->author loopbacks have occurred for
// the current plan. If we've already bounced back twice with no progress, the plan
// likely contains unachievable instructions.
{
    let backtracks = _execution_state.phase.replan_backtracks;
    if backtracks >= 2 {
        tracing::warn!(
            "data_engineer: escalating to plan revision after {} validate loopbacks (phase={})",
            backtracks,
            phase.as_str()
        );
        let violation = crate::progress_controller::PlanViolation::new(
            phase,
            None,
            format!(
                "Validation has failed {} consecutive times with the same or similar errors. \
                 The author phase was unable to resolve the issue, suggesting the plan \
                 itself contains unachievable instructions.\n\nLatest error: {}",
                backtracks,
                brief
            ),
        );
        crate::phase_contract::commit_plan_revision_loopback(
            thread_store, thread_id, phase, vec![violation],
            crate::progress_controller::PlanRevisionStrategy::Rewrite,
        ).await?;
        return Ok(PhaseExecutorOutcome::TransitionCommitted);
    }
}

// Validation failed -> go back to corresponding author phase.
let trigger_step_idx = thread_state_step_count.saturating_sub(1);
let to_phase = if phase == Phase::CleanseValidate {
    Phase::CleanseAuthor
} else {
    Phase::ModelAuthor
};
// Attach authoritative schema facts for the next authoring turn. This ensures the LLM
// never needs to guess relation columns after a deterministic validate failure.
let dialect = crate::facts::SqlDialect(
    obs.observation
        .get("dialect")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown SQL dialect")
        .to_string(),
);
let facts_bundle = crate::facts::build_validate_fail_facts(
    &actx,
    dialect,
    &obs,
    crate::facts::FactsScope::ValidateFail,
    crate::facts::FactsLimits::for_scope(
        crate::facts::FactsScope::ValidateFail,
    ),
)
.await;
// Best-effort persist into the active plan snapshot for reuse in subsequent authoring turns.
// Keep bounded to avoid unbounded plan growth.
if phase == Phase::CleanseValidate {
    if let Some(mut p) =
        crate::plan::load_cleanse_plan(&actx).await
    {
        append_validate_fail_facts(&mut p.project_snapshot, &facts_bundle, 5);
        crate::plan::save_cleanse_plan(&actx, &p)
            .await
            .map_err(|e| {
                format!(
                    "failed to persist cleanse validate-failure facts bundle: {e}"
                )
            })?;
    }
} else {
    if let Some(mut p) =
        crate::plan::load_model_plan(&actx).await
    {
        append_validate_fail_facts(&mut p.project_snapshot, &facts_bundle, 5);
        crate::plan::save_model_plan(&actx, &p)
            .await
            .map_err(|e| {
                format!(
                    "failed to persist model validate-failure facts bundle: {e}"
                )
            })?;
    }
}

crate::phase_contract::commit_phase_decision(
    &thread_store,
    thread_id,
    Some(phase),
    crate::phase_contract::PhaseDecision::loopback(
        to_phase,
        Some(PhaseReasonCode::ValidateFail),
        Some(crate::phase_reason_detail::to_value(
            &crate::phase_reason_detail::ValidateFailDetail {
                dbt_validate_observation: obs.observation.clone(),
                dbt_validate_step_idx: trigger_step_idx,
                errors: errs,
                facts_bundle: serde_json::to_value(&facts_bundle).unwrap_or(serde_json::Value::Null),
            },
        )),
    ),
)
.await?;
return Ok(PhaseExecutorOutcome::TransitionCommitted);
                
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_pass_transition_decision_is_typed() {
        let incomplete = crate::plan::PlanCompletionSnapshot {
            all_done: false,
            pending_count: 2,
            pending_refs: vec![],
        };
        let complete = crate::plan::PlanCompletionSnapshot {
            all_done: true,
            pending_count: 0,
            pending_refs: vec![],
        };
        assert_eq!(
            DataEngineerSuite::decide_validate_pass_transition(Some(&incomplete)),
            ValidatePassTransition::ToAuthoring
        );
        assert_eq!(
            DataEngineerSuite::decide_validate_pass_transition(Some(&complete)),
            ValidatePassTransition::ToReview
        );
        assert_eq!(
            DataEngineerSuite::decide_validate_pass_transition(None),
            ValidatePassTransition::ToReview
        );
    }
}
