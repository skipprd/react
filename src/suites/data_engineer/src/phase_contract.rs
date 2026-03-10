use crate::domain_types::{GuardBlockKind, PhaseReasonCode};
use react_core::session::ThreadStore;

use crate::control_flow::{Phase, TransitionIntent};

#[derive(Clone, Debug)]
pub struct PhaseDecision {
    pub to: Phase,
    pub intent: TransitionIntent,
    pub reason_code: Option<PhaseReasonCode>,
    pub reason_detail: Option<serde_json::Value>,
}

impl PhaseDecision {
    pub fn forward(
        to: Phase,
        reason_code: Option<PhaseReasonCode>,
        reason_detail: Option<serde_json::Value>,
    ) -> Self {
        Self {
            to,
            intent: TransitionIntent::Forward,
            reason_code,
            reason_detail,
        }
    }

    pub fn loopback(
        to: Phase,
        reason_code: Option<PhaseReasonCode>,
        reason_detail: Option<serde_json::Value>,
    ) -> Self {
        Self {
            to,
            intent: TransitionIntent::Loopback,
            reason_code,
            reason_detail,
        }
    }

    pub fn annotation(
        phase: Phase,
        reason_code: Option<PhaseReasonCode>,
        reason_detail: Option<serde_json::Value>,
    ) -> Self {
        Self {
            to: phase,
            intent: TransitionIntent::Annotation,
            reason_code,
            reason_detail,
        }
    }
}

pub async fn commit_phase_decision(
    thread_store: &ThreadStore,
    thread_id: &str,
    from_phase: Option<Phase>,
    decision: PhaseDecision,
) -> Result<(), String> {
    let PhaseDecision {
        to,
        intent,
        reason_code,
        reason_detail,
    } = decision;
    crate::transition_dispatcher::apply_phase_directive(
        thread_store,
        thread_id,
        Some(crate::env_util::DEFAULT_AGENT_NAME.to_string()),
        from_phase,
        crate::transition_dispatcher::PhaseDirective::Transition {
            to,
            intent,
            reason_code,
            reason_detail,
        },
    )
    .await
    .map_err(|e| e.to_string())
}

pub async fn commit_guard_block(
    thread_store: &ThreadStore,
    thread_id: &str,
    phase: Phase,
    kind: GuardBlockKind,
    reason: impl Into<String>,
) -> Result<(), String> {
    crate::transition_dispatcher::apply_phase_directive(
        thread_store,
        thread_id,
        Some(crate::env_util::DEFAULT_AGENT_NAME.to_string()),
        Some(phase),
        crate::transition_dispatcher::PhaseDirective::Block {
            phase,
            kind,
            reason: reason.into(),
        },
    )
    .await
    .map_err(|e| e.to_string())
}

pub async fn commit_plan_revision_loopback(
    thread_store: &ThreadStore,
    thread_id: &str,
    from_phase: Phase,
    violations: Vec<crate::progress_controller::PlanViolation>,
    strategy: crate::progress_controller::PlanRevisionStrategy,
) -> Result<(), String> {
    let track = crate::track_spec::TrackKind::from_any_phase(from_phase)
        .ok_or_else(|| {
            format!(
                "PlanRevisionRequested from phase '{}' which has no associated plan track",
                from_phase.as_str()
            )
        })?;
    crate::state_manager::mutate_execution_state(
        &thread_store.control_store(),
        thread_id,
        |es| es.set_pending_plan_revision(violations, strategy),
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())?;
    commit_phase_decision(
        thread_store,
        thread_id,
        Some(from_phase),
        PhaseDecision::loopback(
            track.plan_phase(),
            Some(PhaseReasonCode::PlanRevisionRequested),
            None,
        ),
    )
    .await
}

pub fn plan_status_reason_detail(status: crate::plan_types::PlanStatus) -> serde_json::Value {
    serde_json::json!({ "status": status.as_str() })
}
