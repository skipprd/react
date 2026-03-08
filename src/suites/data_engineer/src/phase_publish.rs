use crate::phase_contract::{commit_phase_decision, PhaseDecision};
use crate::{control_flow, tools, DataEngineerSuite, PhaseExecutorOutcome};
use react_core::suite::{FlowFrame, FlowKind, SuiteCtx};
use crate::domain_types::PhaseReasonCode;
use react_core::session::ThreadStore;

fn check_publish_retry_limit(
    es: &mut crate::progress_controller::ExecutionState,
    _thread_store: &ThreadStore,
    _thread_id: &str,
    kind: crate::progress_controller::PublishRetryKind,
    error_prefix: &str,
) -> (usize, Option<Result<PhaseExecutorOutcome, String>>) {
    let retry_limit = crate::controller_kernel::publish_retry_limit();
    let retry_count = es.bump_publish_retry(kind, retry_limit);
    let err = if retry_count > retry_limit {
        Some(Err(format!(
            "{error_prefix}: retries={retry_count}"
        )))
    } else {
        None
    };
    (retry_count, err)
}

impl DataEngineerSuite {
    pub(super) async fn execute_publish_await_approval_phase(
        thread_store: &ThreadStore,
        thread_id: &str,
        _sctx: &SuiteCtx,
    ) -> Result<PhaseExecutorOutcome, String> {
        let mut es = crate::progress_controller::ExecutionState::load_strict(
            &thread_store.control_store(),
            thread_id,
        )
        .await?
        .unwrap_or_else(crate::progress_controller::ExecutionState::new);
        if let Err(reason) = crate::progress_controller::gate_publish_progress(
            &es,
            control_flow::Phase::PublishAwaitApproval,
        ) {
            let (_retry_count, err) = check_publish_retry_limit(
                &mut es,
                thread_store,
                thread_id,
                crate::progress_controller::PublishRetryKind::AwaitApprovalLoop,
                &format!("publish_await_approval_not_converged_after_retries; reason={reason}"),
            );
            if let Some(err) = err {
                es.save(&thread_store.control_store(), thread_id).await.map_err(|e| {
                    format!("failed to persist publish await-approval retry state: {e}")
                })?;
                return err;
            }
            es.save(&thread_store.control_store(), thread_id).await.map_err(|e| {
                format!("failed to persist publish await-approval retry state: {e}")
            })?;
            return Ok(PhaseExecutorOutcome::StayInPhase);
        }

        es.reset_publish_retry(
            crate::progress_controller::PublishRetryKind::AwaitApprovalLoop,
        );
        es.save(&thread_store.control_store(), thread_id).await.map_err(|e| {
            format!("failed to persist publish approval consumption state: {e}")
        })?;
        let approval_detail = crate::phase_reason_detail::publish_approval_state(
            serde_json::to_value(&es.publish.publish_approval).unwrap_or(serde_json::Value::Null),
        );
        commit_phase_decision(
            thread_store,
            thread_id,
            Some(control_flow::Phase::PublishAwaitApproval),
            PhaseDecision::forward(
                control_flow::Phase::Publish,
                Some(PhaseReasonCode::UserApprovedPublish),
                Some(crate::phase_reason_detail::publish_auto_approved(
                    serde_json::json!({
                        "approval_state": approval_detail
                    }),
                )),
            ),
        )
        .await?;
        Ok(PhaseExecutorOutcome::TransitionCommitted)
    }

    pub(super) async fn execute_publish_phase(
        thread_store: &ThreadStore,
        thread_id: &str,
        sctx: &SuiteCtx,
    ) -> Result<PhaseExecutorOutcome, String> {
        let mut es = crate::progress_controller::ExecutionState::load_strict(
            &thread_store.control_store(),
            thread_id,
        )
        .await?
        .unwrap_or_else(crate::progress_controller::ExecutionState::new);
        if let Err(reason) =
            crate::progress_controller::gate_publish_progress(&es, control_flow::Phase::Publish)
        {
            return Err(reason);
        }
        let actx = Self::agent_tool_ctx(thread_id, sctx);
        let tool = tools::publish_dbt_to_provider::PublishDbtToProviderTool {
            datasets: crate::ctx_ext::sctx_datasets(sctx),
            catalog: crate::ctx_ext::sctx_catalog(sctx),
        };
        let obs = control_flow::call_and_record_tool(
            thread_store,
            thread_id,
            Some(crate::env_util::DEFAULT_AGENT_NAME.to_string()),
            &tool,
            serde_json::json!({"confirm": true}),
            &actx,
            600,
        )
        .await;
        let ok = obs.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        let stage = obs.get("stage").and_then(|v| v.as_str()).unwrap_or("");
        if ok && (stage == "published" || stage == "no_change") {
            es.clear_publish_approval();
            es.reset_publish_retries();
            es.save(&thread_store.control_store(), thread_id).await.map_err(|e| {
                format!("failed to persist publish confirmed-success state: {e}")
            })?;
            commit_phase_decision(
                thread_store,
                thread_id,
                Some(control_flow::Phase::Publish),
                PhaseDecision::forward(
                    control_flow::Phase::PostPublishReview,
                    Some(PhaseReasonCode::PublishConfirmedSuccess),
                    Some(crate::phase_reason_detail::publish_observation(
                        obs.clone(),
                    )),
                ),
            )
            .await?;
            return Ok(PhaseExecutorOutcome::TransitionCommitted);
        }
        if ok && stage == "await_approval" {
            let (retry_count, err) = check_publish_retry_limit(
                &mut es,
                thread_store,
                thread_id,
                crate::progress_controller::PublishRetryKind::AwaitApprovalLoop,
                "publish_await_approval_not_converged_after_retries",
            );
            if let Some(err) = err {
                es.clear_publish_approval();
                es.save(&thread_store.control_store(), thread_id).await.map_err(|e| {
                    format!("failed to persist publish approval fallback state: {e}")
                })?;
                return err;
            }
            es.clear_publish_approval();
            es.save(&thread_store.control_store(), thread_id).await.map_err(|e| {
                format!("failed to persist publish approval fallback state: {e}")
            })?;
            commit_phase_decision(
                thread_store,
                thread_id,
                Some(control_flow::Phase::Publish),
                PhaseDecision::loopback(
                    control_flow::Phase::PublishAwaitApproval,
                    Some(PhaseReasonCode::PublishFail),
                    Some(crate::phase_reason_detail::publish_failure(
                        obs,
                        retry_count,
                    )),
                ),
            )
            .await?;
            return Ok(PhaseExecutorOutcome::TransitionCommitted);
        }
        let (retry_count, err) = check_publish_retry_limit(
            &mut es,
            thread_store,
            thread_id,
            crate::progress_controller::PublishRetryKind::PublishFailureLoop,
            "publish_confirmed_failure_not_converged_after_retries",
        );
        if let Some(err) = err {
            es.clear_publish_approval();
            es.save(&thread_store.control_store(), thread_id).await.map_err(|e| {
                format!("failed to persist publish confirmed-failure state: {e}")
            })?;
            return err;
        }
        es.clear_publish_approval();
        es.save(&thread_store.control_store(), thread_id).await.map_err(|e| {
            format!("failed to persist publish confirmed-failure state: {e}")
        })?;
        commit_phase_decision(
            thread_store,
            thread_id,
            Some(control_flow::Phase::Publish),
            PhaseDecision::loopback(
                control_flow::Phase::ModelAuthor,
                Some(PhaseReasonCode::PublishConfirmedFail),
                Some(crate::phase_reason_detail::publish_failure(
                    obs,
                    retry_count,
                )),
            ),
        )
        .await?;
        Ok(PhaseExecutorOutcome::TransitionCommitted)
    }

    pub(super) fn execute_done_phase(
        out_frames: &mut Vec<FlowFrame>,
    ) -> Result<PhaseExecutorOutcome, String> {
        let mut answer = "Agent flow completed (deterministic phases): cleanse → validate → review → model → validate → review → publish → review.\n".to_string();
        if let Some(last) = out_frames.iter().rev().find_map(|f| match f {
            FlowFrame::Review { text, .. } => Some(text.clone()),
            _ => None,
        }) {
            answer.push_str("\nLatest review summary:\n");
            answer.push_str(&last);
        }
        out_frames.push(FlowFrame::Complete {
            kind: FlowKind::new("generic"),
            payload: serde_json::json!({ "text": answer.clone() }),
            display: Some(answer),
        });
        Ok(PhaseExecutorOutcome::Return(out_frames.clone()))
    }
}

