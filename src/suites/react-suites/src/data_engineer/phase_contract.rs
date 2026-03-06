use crate::data_engineer::domain_types::{GuardBlockKind, PhaseReasonCode};
use react_core::session::ThreadStore;

use crate::data_engineer::control_flow::{Phase, TransitionIntent};

#[derive(Clone, Debug)]
pub enum PhaseDecision {
    Transition {
        to: Phase,
        intent: TransitionIntent,
        reason_code: Option<PhaseReasonCode>,
        reason_detail: Option<serde_json::Value>,
    },
}

impl PhaseDecision {
    pub fn forward(
        to: Phase,
        reason_code: Option<PhaseReasonCode>,
        reason_detail: Option<serde_json::Value>,
    ) -> Self {
        Self::Transition {
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
        Self::Transition {
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
        Self::Transition {
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
    let PhaseDecision::Transition {
        to,
        intent,
        reason_code,
        reason_detail,
    } = decision;
    crate::data_engineer::transition_dispatcher::apply_phase_directive(
        thread_store,
        thread_id,
        Some("agent".to_string()),
        from_phase,
        crate::data_engineer::transition_dispatcher::PhaseDirective::Transition {
            to,
            intent,
            reason_code,
            reason_detail,
        },
    )
    .await
}

pub async fn commit_guard_block(
    thread_store: &ThreadStore,
    thread_id: &str,
    phase: Phase,
    kind: GuardBlockKind,
    reason: impl Into<String>,
) -> Result<(), String> {
    crate::data_engineer::transition_dispatcher::apply_phase_directive(
        thread_store,
        thread_id,
        Some("agent".to_string()),
        Some(phase),
        crate::data_engineer::transition_dispatcher::PhaseDirective::Block {
            phase,
            kind,
            reason: reason.into(),
        },
    )
    .await
}

pub async fn commit_plan_revision_loopback(
    thread_store: &ThreadStore,
    thread_id: &str,
    from_phase: Phase,
    violations: Vec<crate::data_engineer::progress_controller::PlanViolation>,
    strategy: crate::data_engineer::progress_controller::PlanRevisionStrategy,
) -> Result<(), String> {
    let track = crate::data_engineer::track_spec::TrackKind::from_any_phase(from_phase)
        .ok_or_else(|| {
            format!(
                "PlanRevisionRequested from phase '{}' which has no associated plan track",
                from_phase.as_str()
            )
        })?;
    crate::data_engineer::state_manager::mutate_execution_state(
        thread_store,
        thread_id,
        |es| es.set_pending_plan_revision(violations, strategy),
    )
    .await
    .map(|_| ())?;
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

pub fn plan_status_reason_detail<S: std::fmt::Debug>(status: S) -> serde_json::Value {
    serde_json::json!({ "status": format!("{status:?}") })
}
