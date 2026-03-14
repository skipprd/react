use super::*;
use crate::control_flow::Phase;

const MAX_VALIDATE_FAIL_FACTS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValidatePassTransition {
    ToAuthoring,
    ToReview,
}

enum ValidateEscalation {
    LoopbackToAuthor {
        guard_kind: GuardBlockKind,
        reason: String,
        reason_code: PhaseReasonCode,
        detail: serde_json::Value,
    },
    Fatal(String),
}

impl DataEngineerSuite {
    async fn apply_validate_escalation(
        thread_store: &ThreadStore,
        thread_id: &str,
        phase: Phase,
        escalation: ValidateEscalation,
    ) -> Result<PhaseExecutorOutcome, PhaseError> {
        match escalation {
            ValidateEscalation::LoopbackToAuthor {
                guard_kind,
                reason,
                reason_code,
                detail,
            } => crate::retry_budget::guard_block_loopback_to_author(
                thread_store,
                thread_id,
                phase,
                guard_kind,
                reason,
                reason_code,
                Some(detail),
            )
            .await
            .map_err(PhaseError::from),
            ValidateEscalation::Fatal(reason) => Err(PhaseError::Fatal(reason)),
        }
    }

    fn validate_execution_failure_escalation(
        reason: String,
        detail: serde_json::Value,
        reason_code: PhaseReasonCode,
        retry_outcome: crate::retry_budget::SubjectiveRetryOutcome,
    ) -> ValidateEscalation {
        match retry_outcome {
            crate::retry_budget::SubjectiveRetryOutcome::Exhausted(tries) => {
                ValidateEscalation::Fatal(format!(
                    "dbt_validate execution failed {tries} times; manual intervention required.\n\n{reason}"
                ))
            }
            crate::retry_budget::SubjectiveRetryOutcome::WithinBudget(_) => {
                ValidateEscalation::LoopbackToAuthor {
                    guard_kind: GuardBlockKind::ValidateExecutionFailed,
                    reason,
                    reason_code,
                    detail,
                }
            }
        }
    }

    fn validate_precheck_escalation(
        reason: String,
        detail: serde_json::Value,
        retry_outcome: crate::retry_budget::SubjectiveRetryOutcome,
    ) -> ValidateEscalation {
        match retry_outcome {
            crate::retry_budget::SubjectiveRetryOutcome::Exhausted(tries) => {
                ValidateEscalation::Fatal(format!(
                    "pre-validation repair did not converge after {tries} attempts; manual implementation fix required before re-validating.\n\n{reason}"
                ))
            }
            crate::retry_budget::SubjectiveRetryOutcome::WithinBudget(_) => {
                ValidateEscalation::LoopbackToAuthor {
                    guard_kind: GuardBlockKind::PrecheckFailed,
                    reason,
                    reason_code: PhaseReasonCode::PrecheckFailed,
                    detail,
                }
            }
        }
    }

    async fn handle_validate_execution_failure(
        thread_store: &ThreadStore,
        thread_id: &str,
        phase: Phase,
        reason: String,
        detail: serde_json::Value,
        reason_code: PhaseReasonCode,
    ) -> Result<PhaseExecutorOutcome, PhaseError> {
        let retry_outcome = Self::check_subjective_retry_budget(
            thread_store,
            thread_id,
            crate::progress_controller::SubjectiveRetryKind::ValidateExecutionFailed,
        )
        .await?;
        let escalation =
            Self::validate_execution_failure_escalation(reason, detail, reason_code, retry_outcome);
        Self::apply_validate_escalation(thread_store, thread_id, phase, escalation).await
    }
    fn decide_validate_pass_transition(
        completion_snapshot: Option<&crate::plan::PlanCompletionSnapshot>,
    ) -> ValidatePassTransition {
        match completion_snapshot
            .map(|snap| snap.completion_state())
            .unwrap_or(crate::plan::PlanCompletionState::Complete)
        {
            crate::plan::PlanCompletionState::Incomplete => ValidatePassTransition::ToAuthoring,
            crate::plan::PlanCompletionState::Complete => ValidatePassTransition::ToReview,
        }
    }

    async fn reduce_validate_pass_plan_state(
        actx: &AgentCtx,
        phase: Phase,
    ) -> Result<(Option<crate::plan::PlanCompletionSnapshot>, Option<String>), String> {
        let mut completion_snapshot: Option<crate::plan::PlanCompletionSnapshot> = None;
        let mut active_plan_key: Option<String> = None;
        if phase == Phase::CleanseValidate {
            if let Some(mut p) = crate::plan::load_cleanse_plan_any(actx)
                .await
                .map_err(|e| e.to_string())?
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
                crate::plan::save_cleanse_plan(actx, &p)
                    .await
                    .map_err(|e| {
                        format!("failed to persist cleanse plan validate-pass state: {e}")
                    })?;
            }
        } else if let Some(mut p) = crate::plan::load_model_plan_any(actx)
            .await
            .map_err(|e| e.to_string())?
        {
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
            crate::plan::save_model_plan(actx, &p)
                .await
                .map_err(|e| format!("failed to persist model plan validate-pass state: {e}"))?;
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
    ) -> Result<PhaseExecutorOutcome, PhaseError> {
        let actx = Self::agent_tool_ctx(thread_id, sctx);
        {
            let mut es = crate::progress_controller::ExecutionState::load(
                &thread_store.control_store(),
                thread_id,
            )
            .await
            .map_err(|e| format!("failed to load execution state before validate: {e}"))?
            .unwrap_or_else(crate::progress_controller::ExecutionState::new);
            let tier = if phase == Phase::CleanseValidate {
                crate::progress_controller::ExecutionTier::Cleanse
            } else {
                crate::progress_controller::ExecutionTier::Model
            };
            es.enter_validate_mode(tier);
            es.save(&thread_store.control_store(), thread_id)
                .await
                .map_err(|e| format!("failed to persist execution state before validate: {e}"))?;
        }
        // Pre-validate normalization: dedupe/merge repeated model+test definitions to
        // avoid deterministic compile loops before dbt_validate.
        if let Err(e) = crate::schema_policy::normalize_schema_artifacts_for_validate(&actx).await {
            let retry_outcome = Self::check_subjective_retry_budget(
                &thread_store,
                thread_id,
                crate::progress_controller::SubjectiveRetryKind::ValidatePrecheckFailed,
            )
            .await?;
            let escalation = Self::validate_precheck_escalation(
                format!(
                    "Pre-validation normalization failed; fix DBT YAML artifacts before re-validating.\n\n{e}"
                ),
                serde_json::json!({ "error": e }),
                retry_outcome,
            );
            return Self::apply_validate_escalation(&thread_store, thread_id, phase, escalation)
                .await;
        }

        // Cheap structural prechecks: fail fast on malformed/duplicated schema artifacts
        // instead of burning a full dbt_validate cycle.
        if let Err(e) = crate::schema_policy::prevalidate_dbt_schema_artifacts(&actx).await {
            let retry_outcome = Self::check_subjective_retry_budget(
                &thread_store,
                thread_id,
                crate::progress_controller::SubjectiveRetryKind::ValidatePrecheckFailed,
            )
            .await?;
            let escalation = Self::validate_precheck_escalation(
                format!(
                    "Pre-validation failed; fix DBT YAML artifacts before re-validating.\n\n{e}"
                ),
                serde_json::json!({ "error": e }),
                retry_outcome,
            );
            return Self::apply_validate_escalation(&thread_store, thread_id, phase, escalation)
                .await;
        }

        // Deterministic full validate (NO repair loop / no mutation).
        let args_full = serde_json::json!({"build": true});
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
        validate_ctx.set("build", serde_json::Value::Bool(true));
        let obs = {
            let meta = react_core::session::ToolStepMeta {
                agent: "agent".to_string(),
                phase: phase.as_str().to_string(),
                name: "dbt_validate".to_string(),
                clean_name: "Validate DBT".to_string(),
                args: args_full,
                ctx: Some(validate_ctx),
            };
            match thread_store
                .run_observed(
                    thread_id,
                    meta,
                    || async {
                        control_flow::DeterministicDbtValidateOnce::run(&actx, true, false, None)
                            .await
                    },
                    |contract| Ok(contract.observation.clone()),
                )
                .await
            {
                Ok(contract) => contract,
                Err(e) => {
                    let is_transient = crate::failure_text::is_infra_transient(
                        &crate::failure_text::normalize_text(&e),
                    );
                    if is_transient {
                        return Err(PhaseError::Fatal(format!(
                    "dbt_validate execution failed due to transient infrastructure error: {e}"
                )));
                    }
                    tracing::warn!(
                        "data_engineer: validate execution failed (thread_id={} phase={}): {}",
                        thread_id,
                        phase.as_str(),
                        e
                    );
                    return Self::handle_validate_execution_failure(
                        thread_store,
                        thread_id,
                        phase,
                        format!("dbt_validate execution failed: {}", e),
                        serde_json::json!({"error": e}),
                        PhaseReasonCode::ValidateExecutionFailed,
                    )
                    .await;
                }
            }
        };

        let validate_event = crate::controller_event::validate_event_from_contract(&obs);
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
                    crate::progress_controller::DataEngineerEvent::ValidatePassed { tier },
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
        let (failure_class, failure_signature, brief, failing_targets, compile_ok, run_ok) =
            match validate_event {
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
                    tracing::warn!(
                        "data_engineer: validate contract error: {} ({}) (thread_id={} phase={})",
                        reason,
                        brief,
                        thread_id,
                        phase.as_str()
                    );
                    return Self::handle_validate_execution_failure(
                        thread_store,
                        thread_id,
                        phase,
                        format!("validate_outcome_v2_contract_error: {} ({})", reason, brief),
                        serde_json::json!({"reason": reason, "brief": brief}),
                        PhaseReasonCode::ValidateContractError,
                    )
                    .await;
                }
                crate::controller_event::ControllerEvent::ValidatePassed => {
                    // Covered by the success branch above.
                    unreachable!("validate pass should have continued above")
                }
            };
        if matches!(
            failure_class,
            crate::failure_kind::FailureKind::InfraTransient
        ) {
            return Err(PhaseError::Fatal(format!(
        "dbt_validate failed due to a transient infrastructure error (service outage, throttling, or network issue). \
         This is not a code defect — retry after the upstream service recovers.\n\n{}",
        brief
    )));
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
            let mut failing_models: Vec<crate::progress_controller::FailedModelRef> = Vec::new();
            for t in failing_targets.iter() {
                let rel = t.target_path.as_str().to_string();
                let key = crate::project_fs::join_storage_key(&actx, &rel);
                let materialization = match actx.storage().get_bytes(&key).await {
                    Ok(_) => crate::progress_controller::RepairTargetMaterialization::Existing,
                    Err(_) => crate::progress_controller::RepairTargetMaterialization::Missing,
                };
                failing_models.push(crate::progress_controller::FailedModelRef {
                    name: t.node_id.clone(),
                    file: rel,
                    error: None,
                    materialization,
                });
            }
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
            crate::facts::FactsLimits::for_scope(crate::facts::FactsScope::ValidateFail),
        )
        .await;
        // Best-effort persist into the active plan snapshot for reuse in subsequent authoring turns.
        // Keep bounded to avoid unbounded plan growth.
        if phase == Phase::CleanseValidate {
            if let Some(mut p) = crate::plan::load_cleanse_plan(&actx).await? {
                p.project_snapshot.validate_fail_facts.push(facts_bundle.clone());
                if p.project_snapshot.validate_fail_facts.len() > MAX_VALIDATE_FAIL_FACTS {
                    p.project_snapshot.validate_fail_facts.remove(0);
                }
                crate::plan::save_cleanse_plan(&actx, &p)
                    .await
                    .map_err(|e| {
                        format!("failed to persist cleanse validate-failure facts bundle: {e}")
                    })?;
            }
        } else {
            if let Some(mut p) = crate::plan::load_model_plan(&actx).await? {
                p.project_snapshot.validate_fail_facts.push(facts_bundle.clone());
                if p.project_snapshot.validate_fail_facts.len() > MAX_VALIDATE_FAIL_FACTS {
                    p.project_snapshot.validate_fail_facts.remove(0);
                }
                crate::plan::save_model_plan(&actx, &p).await.map_err(|e| {
                    format!("failed to persist model validate-failure facts bundle: {e}")
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
                        facts_bundle: serde_json::to_value(&facts_bundle)
                            .unwrap_or(serde_json::Value::Null),
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

    #[test]
    fn validate_execution_failure_exhaustion_is_not_plan_rewrite() {
        let escalation = DataEngineerSuite::validate_execution_failure_escalation(
            "dbt_validate execution failed".to_string(),
            serde_json::json!({"error":"boom"}),
            PhaseReasonCode::ValidateExecutionFailed,
            crate::retry_budget::SubjectiveRetryOutcome::Exhausted(3),
        );
        match escalation {
            ValidateEscalation::Fatal(reason) => {
                assert!(reason.contains("manual intervention required"));
            }
            _ => panic!("expected fatal escalation"),
        }
    }

    #[test]
    fn validate_precheck_exhaustion_is_not_plan_rewrite() {
        let escalation = DataEngineerSuite::validate_precheck_escalation(
            "Pre-validation failed".to_string(),
            serde_json::json!({"error":"yaml"}),
            crate::retry_budget::SubjectiveRetryOutcome::Exhausted(2),
        );
        match escalation {
            ValidateEscalation::Fatal(reason) => {
                assert!(reason.contains("manual implementation fix required"));
            }
            _ => panic!("expected fatal escalation"),
        }
    }
}
