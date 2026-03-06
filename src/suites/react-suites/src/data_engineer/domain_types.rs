use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Proceed,
    PatchImpl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTier {
    Silver,
    Gold,
    Unknown,
}

impl Default for ReviewTier {
    fn default() -> Self {
        ReviewTier::Unknown
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewDecisionMeta {
    pub decision: ReviewDecision,
    #[serde(default)]
    pub tier: ReviewTier,
    #[serde(default)]
    pub dataset_ids: Vec<String>,
    #[serde(default)]
    pub review_ref: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseReasonCode {
    PhaseSet,
    PreflightStart,
    PreflightOk,
    PlanApproved,
    PlanAutoApproved,
    PlanAlreadyApproved,
    PlanMissing,
    PlanNotApproved,
    PlanInvalidEmpty,
    PlanPrunedEmpty,
    PlanSemanticInvalid,
    WorkGroupValidate,
    PlanTasksDone,
    NoWorkAllDone,
    AuthoringComplete,
    PrecheckFailed,
    ValidatePassToReview,
    ValidatePassToAuthoring,
    ValidateFail,
    ReviewProceed,
    ReviewPatchImpl,
    ReviewProjectSummary,
    ReviewBatch,
    ReviewFinalUnify,
    UserApprovedPublish,
    PublishSuccess,
    PublishFail,
    PublishConfirmedSuccess,
    PublishConfirmedFail,
    PlanRevisionRequested,
    PhaseBlocked,
}

impl PhaseReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            PhaseReasonCode::PhaseSet => "phase_set",
            PhaseReasonCode::PreflightStart => "preflight_start",
            PhaseReasonCode::PreflightOk => "preflight_ok",
            PhaseReasonCode::PlanApproved => "plan_approved",
            PhaseReasonCode::PlanAutoApproved => "plan_auto_approved",
            PhaseReasonCode::PlanAlreadyApproved => "plan_already_approved",
            PhaseReasonCode::PlanMissing => "plan_missing",
            PhaseReasonCode::PlanNotApproved => "plan_not_approved",
            PhaseReasonCode::PlanInvalidEmpty => "plan_invalid_empty",
            PhaseReasonCode::PlanPrunedEmpty => "plan_pruned_empty",
            PhaseReasonCode::PlanSemanticInvalid => "plan_semantic_invalid",
            PhaseReasonCode::WorkGroupValidate => "work_group_validate",
            PhaseReasonCode::PlanTasksDone => "plan_tasks_done",
            PhaseReasonCode::NoWorkAllDone => "no_work_all_done",
            PhaseReasonCode::AuthoringComplete => "authoring_complete",
            PhaseReasonCode::PrecheckFailed => "precheck_failed",
            PhaseReasonCode::ValidatePassToReview => "validate_pass_to_review",
            PhaseReasonCode::ValidatePassToAuthoring => "validate_pass_to_authoring",
            PhaseReasonCode::ValidateFail => "validate_fail",
            PhaseReasonCode::ReviewProceed => "review_proceed",
            PhaseReasonCode::ReviewPatchImpl => "review_patch_impl",
            PhaseReasonCode::ReviewProjectSummary => "review_project_summary",
            PhaseReasonCode::ReviewBatch => "review_batch",
            PhaseReasonCode::ReviewFinalUnify => "review_final_unify",
            PhaseReasonCode::UserApprovedPublish => "user_approved_publish",
            PhaseReasonCode::PublishSuccess => "publish_success",
            PhaseReasonCode::PublishFail => "publish_fail",
            PhaseReasonCode::PublishConfirmedSuccess => "publish_confirmed_success",
            PhaseReasonCode::PublishConfirmedFail => "publish_confirmed_fail",
            PhaseReasonCode::PlanRevisionRequested => "plan_revision_requested",
            PhaseReasonCode::PhaseBlocked => "phase_blocked",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardBlockKind {
    PlanJsonInvalid,
    PlanGrounding,
    PlanSemanticInvalid,
    PlanDesignCritique,
    BatchLocked,
    AuthoringCompletion,
    AuthoringToValidate,
    MissingGoldModels,
    PrecheckFailed,
    MissingThreadStep,
}

impl GuardBlockKind {
    pub fn as_str(self) -> &'static str {
        match self {
            GuardBlockKind::PlanJsonInvalid => "plan_json_invalid",
            GuardBlockKind::PlanGrounding => "plan_grounding",
            GuardBlockKind::PlanSemanticInvalid => "plan_semantic_invalid",
            GuardBlockKind::PlanDesignCritique => "plan_design_critique",
            GuardBlockKind::BatchLocked => "batch_locked",
            GuardBlockKind::AuthoringCompletion => "authoring_completion",
            GuardBlockKind::AuthoringToValidate => "authoring_to_validate",
            GuardBlockKind::MissingGoldModels => "missing_gold_models",
            GuardBlockKind::PrecheckFailed => "precheck_failed",
            GuardBlockKind::MissingThreadStep => "missing_thread_step",
        }
    }
}
