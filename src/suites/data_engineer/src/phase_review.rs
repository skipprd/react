use crate::domain_types::{PhaseReasonCode, ReviewDecision, ReviewDecisionMeta, ReviewTier};
use crate::phase_contract::{commit_phase_decision, PhaseDecision};
use crate::review_batched;
use crate::{control_flow, DataEngineerSuite, PhaseError, PhaseExecutorOutcome};
use react_core::session::ThreadStore;
use react_core::suite::{FlowFrame, FlowKind, SuiteCtx};

fn effective_review_tier(phase: control_flow::Phase, tier: ReviewTier) -> ReviewTier {
    if tier != ReviewTier::Unknown {
        return tier;
    }
    match phase {
        control_flow::Phase::CleanseReview => ReviewTier::Silver,
        control_flow::Phase::ModelReview | control_flow::Phase::PostPublishReview => {
            ReviewTier::Gold
        }
        _ => ReviewTier::Unknown,
    }
}

fn patch_impl_target_phase(phase: control_flow::Phase, tier: ReviewTier) -> control_flow::Phase {
    match effective_review_tier(phase, tier) {
        ReviewTier::Silver => control_flow::Phase::CleanseAuthor,
        ReviewTier::Gold | ReviewTier::Unknown => control_flow::Phase::ModelAuthor,
    }
}

impl DataEngineerSuite {
    pub(super) async fn execute_review_phase(
        thread_store: &ThreadStore,
        thread_id: &str,
        phase: control_flow::Phase,
        question: &str,
        sctx: &SuiteCtx,
        execution_state: &crate::progress_controller::ExecutionState,
        thread_state_step_count: usize,
        out_frames: &mut Vec<FlowFrame>,
    ) -> Result<PhaseExecutorOutcome, PhaseError> {
        let review_q = Self::build_review_question_with_context(question, phase, execution_state);
        let frames = match review_batched::run_batched_review(thread_id, &review_q, phase, sctx)
            .await
        {
            Ok(f) => f,
            Err(e) => {
                let validate_passed = execution_state
                    .telemetry
                    .last_validate
                    .as_ref()
                    .and_then(|lv| lv.ok)
                    == Some(true);
                if validate_passed {
                    tracing::warn!(
                        "data_engineer: review infra failure (phase={}) but validate already passed; proceeding. error={}",
                        phase.as_str(),
                        e
                    );
                    let next = match phase {
                        control_flow::Phase::CleanseReview => control_flow::Phase::ModelPlan,
                        control_flow::Phase::ModelReview => {
                            control_flow::Phase::PublishAwaitApproval
                        }
                        control_flow::Phase::PostPublishReview => control_flow::Phase::Done,
                        _ => control_flow::Phase::Done,
                    };
                    out_frames.push(FlowFrame::Review {
                        text: format!(
                            "Review skipped due to infrastructure error (validate passed): {}",
                            e
                        ),
                        meta: None,
                    });
                    commit_phase_decision(
                        thread_store,
                        thread_id,
                        Some(phase),
                        PhaseDecision::forward(
                            next,
                            Some(PhaseReasonCode::ReviewProceed),
                            Some(serde_json::json!({
                                "review_skipped_reason": "infra_failure_after_validate_pass",
                                "error": e.to_string(),
                            })),
                        ),
                    )
                    .await?;
                    return Ok(PhaseExecutorOutcome::TransitionCommitted);
                }
                return Err(PhaseError::from(e));
            }
        };
        let first = frames.into_iter().next().unwrap_or(FlowFrame::Complete {
            kind: FlowKind::new("generic"),
            payload: serde_json::json!({ "text": "" }),
            display: None,
        });
        let (answer, decision_meta_v) = match first {
            FlowFrame::Complete {
                payload, display, ..
            } => {
                let ans = display
                    .or_else(|| {
                        payload
                            .get("text")
                            .and_then(|x| x.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();
                let mv = payload.get("meta").cloned();
                (ans, mv)
            }
            other => return Ok(PhaseExecutorOutcome::Return(vec![other])),
        };

        let trigger_step_idx = thread_state_step_count.saturating_sub(1);
        let trigger_step = serde_json::json!({
            "phase": phase.as_str(),
            "phase_reason_code": execution_state
                .phase
                .phase_reason_code
                .map(|c| c.as_str().to_string()),
            "phase_reason_detail": execution_state.phase.phase_reason_detail.clone(),
        });

        let mut meta: ReviewDecisionMeta = match decision_meta_v {
            Some(v) => serde_json::from_value(v).map_err(|e| {
                PhaseError::ToolContractViolation(format!("review meta did not match schema: {e}"))
            })?,
            None => {
                return Err(PhaseError::ToolContractViolation(
                    "review meta missing from review result".to_string(),
                ))
            }
        };
        let review_ref_from_trigger = trigger_step
            .get("phase_reason_detail")
            .cloned()
            .filter(|v| !v.is_null())
            .and_then(|v| {
                serde_json::from_value::<crate::phase_reason_detail::ReviewDecisionTransitionDetail>(v).ok()
            })
            .and_then(|detail| detail.meta.review_ref);
        if meta.review_ref.is_none() {
            meta.review_ref = review_ref_from_trigger;
        }
        let review_retry_count;
        let is_patch_impl = meta.decision == ReviewDecision::PatchImpl;
        if is_patch_impl {
            use crate::retry_budget::SubjectiveRetryOutcome;
            match Self::check_subjective_retry_budget(
                thread_store,
                thread_id,
                crate::progress_controller::SubjectiveRetryKind::ReviewPatchImpl,
            )
            .await?
            {
                SubjectiveRetryOutcome::Exhausted(tries) => {
                    let evidence = format!(
                        "Review PatchImpl looped {} times without convergence. The plan may contain unachievable requirements.",
                        tries,
                    );
                    tracing::warn!(
                        "data_engineer: escalating to plan revision phase={} reason={}",
                        phase.as_str(),
                        evidence
                    );
                    Self::clear_subjective_retries_matching(thread_store, thread_id, |k| {
                        matches!(
                            k,
                            crate::progress_controller::SubjectiveRetryKind::ReviewPatchImpl
                        )
                    })
                    .await?;
                    let violation =
                        crate::progress_controller::PlanViolation::new(phase, None, evidence);
                    crate::phase_contract::commit_plan_revision_loopback(
                        thread_store,
                        thread_id,
                        phase,
                        vec![violation],
                        crate::progress_controller::PlanRevisionStrategy::Rewrite,
                    )
                    .await?;
                    return Ok(PhaseExecutorOutcome::TransitionCommitted);
                }
                SubjectiveRetryOutcome::WithinBudget(tries) => {
                    review_retry_count = tries;
                }
            }
        } else {
            review_retry_count = 0;
            Self::clear_subjective_retries_matching(thread_store, thread_id, |k| {
                matches!(
                    k,
                    crate::progress_controller::SubjectiveRetryKind::ReviewPatchImpl
                )
            })
            .await?;
        }
        out_frames.push(FlowFrame::Review {
            text: answer.clone(),
            meta: Some(
                serde_json::to_value(&meta).map_err(|e| {
                    PhaseError::Fatal(format!("failed to serialize review meta: {e}"))
                })?,
            ),
        });

        let reason_detail = crate::phase_reason_detail::review_decision_transition(
            phase.as_str(),
            meta.clone(),
            meta.decision,
            false,
            false,
            review_retry_count,
            trigger_step_idx,
            trigger_step,
        );

        match meta.decision {
            ReviewDecision::Proceed => {
                crate::state_manager::mutate_execution_state(
                    &thread_store.control_store(),
                    thread_id,
                    |es| es.clear_pending_patch_impl(),
                )
                .await
                .map(|_| ())?;
                let next = match phase {
                    control_flow::Phase::CleanseReview => control_flow::Phase::ModelPlan,
                    control_flow::Phase::ModelReview => control_flow::Phase::PublishAwaitApproval,
                    control_flow::Phase::PostPublishReview => control_flow::Phase::Done,
                    _ => control_flow::Phase::Done,
                };
                if next == control_flow::Phase::PublishAwaitApproval {
                    let mut st = crate::progress_controller::ExecutionState::load_strict(
                        &thread_store.control_store(),
                        thread_id,
                    )
                    .await?
                    .unwrap_or_else(crate::progress_controller::ExecutionState::new);
                    st.set_publish_approval(
                        crate::progress_controller::PublishApprovalDecision::Approved,
                    );
                    st.save(&thread_store.control_store(), thread_id).await.map_err(|e| {
                        format!(
                            "failed to persist explicit publish approval on model review proceed: {e}"
                        )
                    })?;
                }
                commit_phase_decision(
                    thread_store,
                    thread_id,
                    Some(phase),
                    PhaseDecision::forward(
                        next,
                        Some(PhaseReasonCode::ReviewProceed),
                        Some(reason_detail),
                    ),
                )
                .await?;
                Ok(PhaseExecutorOutcome::TransitionCommitted)
            }
            ReviewDecision::PatchImpl => {
                let back = patch_impl_target_phase(phase, meta.tier);
                crate::state_manager::mutate_execution_state(
                    &thread_store.control_store(),
                    thread_id,
                    |es| es.set_pending_patch_impl_intent(back),
                )
                .await
                .map(|_| ())?;
                commit_phase_decision(
                    thread_store,
                    thread_id,
                    Some(phase),
                    PhaseDecision::loopback(
                        back,
                        Some(PhaseReasonCode::ReviewPatchImpl),
                        Some(reason_detail),
                    ),
                )
                .await?;
                Ok(PhaseExecutorOutcome::TransitionCommitted)
            }
            ReviewDecision::PlanChange => {
                crate::state_manager::mutate_execution_state(
                    &thread_store.control_store(),
                    thread_id,
                    |es| es.clear_pending_patch_impl(),
                )
                .await
                .map(|_| ())?;
                let cleaned = DataEngineerSuite::strip_meta_line(&answer);
                let evidence = {
                    let trimmed = cleaned.trim();
                    let max = 1_000usize;
                    if trimmed.len() > max {
                        format!("{}...", &trimmed[..max])
                    } else {
                        trimmed.to_string()
                    }
                };
                let violation = crate::progress_controller::PlanViolation::new(
                    phase,
                    None,
                    format!("Review requested plan change: {evidence}"),
                );
                crate::phase_contract::commit_plan_revision_loopback(
                    thread_store,
                    thread_id,
                    phase,
                    vec![violation],
                    crate::progress_controller::PlanRevisionStrategy::Rewrite,
                )
                .await?;
                Ok(PhaseExecutorOutcome::TransitionCommitted)
            }
        }
    }
}
