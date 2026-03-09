use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::track_spec::TrackKind;
use crate::plan_types::PlanStatus;
use crate::domain_types::ReviewDecision;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoApprovalSource {
    NewDraftPlan,
    ExistingDraftPlan,
}

impl std::fmt::Display for AutoApprovalSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NewDraftPlan => f.write_str("new_draft_plan"),
            Self::ExistingDraftPlan => f.write_str("existing_draft_plan"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidateToAuthoringSignal {
    PendingChecklist,
    IncompleteWork,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidateToAuthoringNextAction {
    ResumeAuthoringForRemainingPlanWork,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanAutoApprovedDetail {
    pub auto_approved_in_agent_mode: bool,
    pub source: AutoApprovalSource,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidatePassToAuthoringDetail {
    pub signal: String,
    pub plan_key: Option<String>,
    pub pending_count: usize,
    pub pending_refs: Value,
    pub dbt_validate_observation: Value,
    pub next_action: ValidateToAuthoringNextAction,
    pub audit_acceptance: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidatePassToReviewDetail {
    pub dbt_validate_observation: Value,
    pub dbt_validate_step_idx: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidateFailDetail {
    pub dbt_validate_observation: Value,
    pub dbt_validate_step_idx: usize,
    pub errors: Vec<String>,
    pub facts_bundle: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanMissingDetail {
    pub plan_kind: TrackKind,
    pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanNotApprovedDetail {
    pub status: PlanStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanKeyDetail {
    pub plan_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanSemanticInvalidDetail {
    pub plan_key: String,
    pub reason: String,
    pub audit_acceptance: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanInvalidEmptyDetail {
    pub plan_key: String,
    pub status: PlanStatus,
    pub tasks_len: usize,
    pub batches_len: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CleanseDraftUngroundedDetail {
    pub plan_key: String,
    pub reason: String,
    pub removed_non_raw: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanPrunedEmptyDetail {
    pub plan_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanSemanticInvalidErrorsDetail {
    pub plan_key: String,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewDecisionTransitionDetail {
    pub review_phase: String,
    pub meta: Value,
    pub answer: ReviewDecision,
    pub forced_progress_guard: bool,
    pub forced_progress_by_subjective_retry: bool,
    pub review_subjective_retry_count: usize,
    pub trigger_step_idx: usize,
    pub trigger_step: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishApprovalStateDetail {
    pub approval_state: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishObservationDetail {
    pub publish_observation: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishAutoApprovedDetail {
    pub auto_approved_in_agent_mode: bool,
    pub publish_observation: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishFailureDetail {
    pub publish_observation: Value,
    pub publish_failure_retry_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthoringCompleteInvariantsDetail {
    pub has_dbt_project_yml: bool,
    pub has_any_models: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthoringCompleteGuardStateDetail {
    pub last_validate_failed: bool,
    pub mutated_since_fail: bool,
    pub patched_since_fail: bool,
    pub mutation_failures_since_validate: usize,
    pub probe_required: bool,
    pub probe_satisfied: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthoringCompleteReasonDetail {
    pub invariants: AuthoringCompleteInvariantsDetail,
    pub guard_state: AuthoringCompleteGuardStateDetail,
}

pub fn to_value<T: Serialize>(detail: &T) -> Value {
    react_core::workflow::reason_detail_value(detail)
}

pub fn plan_auto_approved(source: AutoApprovalSource) -> Value {
    to_value(&PlanAutoApprovedDetail {
        auto_approved_in_agent_mode: true,
        source,
    })
}

pub fn review_decision_transition(
    review_phase: impl Into<String>,
    meta: Value,
    answer: ReviewDecision,
    forced_progress_guard: bool,
    forced_progress_by_subjective_retry: bool,
    review_subjective_retry_count: usize,
    trigger_step_idx: usize,
    trigger_step: Value,
) -> Value {
    to_value(&ReviewDecisionTransitionDetail {
        review_phase: review_phase.into(),
        meta,
        answer,
        forced_progress_guard,
        forced_progress_by_subjective_retry,
        review_subjective_retry_count,
        trigger_step_idx,
        trigger_step,
    })
}

pub fn publish_approval_state(approval_state: Value) -> Value {
    to_value(&PublishApprovalStateDetail { approval_state })
}

pub fn publish_observation(publish_observation: Value) -> Value {
    to_value(&PublishObservationDetail {
        publish_observation,
    })
}

pub fn publish_auto_approved(publish_observation: Value) -> Value {
    to_value(&PublishAutoApprovedDetail {
        auto_approved_in_agent_mode: true,
        publish_observation,
    })
}

pub fn publish_failure(
    publish_observation: Value,
    publish_failure_retry_count: usize,
) -> Value {
    to_value(&PublishFailureDetail {
        publish_observation,
        publish_failure_retry_count,
    })
}

pub fn plan_missing(plan_kind: TrackKind, note: impl Into<String>) -> Value {
    to_value(&PlanMissingDetail {
        plan_kind,
        note: note.into(),
    })
}

pub fn plan_not_approved(status: PlanStatus) -> Value {
    to_value(&PlanNotApprovedDetail {
        status,
    })
}

pub fn plan_key(plan_key: impl Into<String>) -> Value {
    to_value(&PlanKeyDetail {
        plan_key: plan_key.into(),
    })
}

