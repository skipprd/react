use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Proceed,
    PatchImpl,
    PlanChange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewDecisionMeta {
    pub decision: ReviewDecision,
    #[serde(default)]
    pub tier: ReviewTier,
    #[serde(default)]
    pub dataset_ids: Vec<String>,
    #[serde(default)]
    pub review_ref: Option<ReviewArtifactRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewArtifactRef {
    pub key: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewSummaryOutput {
    #[serde(default)]
    pub project_notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewBatchOutput {
    #[serde(default)]
    pub findings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewUnifyOutput {
    pub decision: ReviewDecision,
    #[serde(default)]
    pub tier: ReviewTier,
    #[serde(default)]
    pub dataset_ids: Vec<String>,
    pub final_review_text: String,
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
    ValidateExecutionFailed,
    ValidateContractError,
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
            PhaseReasonCode::ValidateExecutionFailed => "validate_execution_failed",
            PhaseReasonCode::ValidateContractError => "validate_contract_error",
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
    ValidateExecutionFailed,
    MissingThreadStep,
    PhaseExecutionError,
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
            GuardBlockKind::ValidateExecutionFailed => "validate_execution_failed",
            GuardBlockKind::MissingThreadStep => "missing_thread_step",
            GuardBlockKind::PhaseExecutionError => "phase_execution_error",
        }
    }
}

/// Project-relative path to a dbt model file that was the target of a validate failure.
pub type ValidateTargetPath = crate::progress_controller::RepairTargetPath;

#[derive(Clone, Debug, PartialEq)]
pub enum ControllerEvent {
    ValidatePassed,
    ValidateFailed {
        class: crate::failure_kind::FailureKind,
        signature: FailureSignature,
        brief: String,
        failing_targets: Vec<ValidateFailingTarget>,
        compile_ok: bool,
        run_ok: bool,
    },
    ValidateContractError {
        reason: String,
        brief: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidateFailingTarget {
    pub node_id: String,
    #[serde(rename = "canonical_path")]
    pub target_path: ValidateTargetPath,
    pub error_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureSignature {
    pub class: crate::failure_kind::FailureKind,
    pub node_id: String,
    #[serde(rename = "canonical_path")]
    pub target_path: ValidateTargetPath,
    pub error_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidateOutcomeV2 {
    pub ok: bool,
    pub compile_ok: bool,
    pub run_ok: bool,
    #[serde(default)]
    pub failing_targets: Vec<ValidateFailingTarget>,
    #[serde(default)]
    pub failure_signature: Option<FailureSignature>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidateObservationContract {
    pub observation: Value,
    pub outcome_v2: ValidateOutcomeV2,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_PHASE_REASON_CODES: [PhaseReasonCode; 33] = [
        PhaseReasonCode::PhaseSet,
        PhaseReasonCode::PreflightStart,
        PhaseReasonCode::PreflightOk,
        PhaseReasonCode::PlanApproved,
        PhaseReasonCode::PlanAutoApproved,
        PhaseReasonCode::PlanAlreadyApproved,
        PhaseReasonCode::PlanMissing,
        PhaseReasonCode::PlanNotApproved,
        PhaseReasonCode::PlanInvalidEmpty,
        PhaseReasonCode::PlanPrunedEmpty,
        PhaseReasonCode::PlanSemanticInvalid,
        PhaseReasonCode::WorkGroupValidate,
        PhaseReasonCode::PlanTasksDone,
        PhaseReasonCode::NoWorkAllDone,
        PhaseReasonCode::AuthoringComplete,
        PhaseReasonCode::PrecheckFailed,
        PhaseReasonCode::ValidateExecutionFailed,
        PhaseReasonCode::ValidateContractError,
        PhaseReasonCode::ValidatePassToReview,
        PhaseReasonCode::ValidatePassToAuthoring,
        PhaseReasonCode::ValidateFail,
        PhaseReasonCode::ReviewProceed,
        PhaseReasonCode::ReviewPatchImpl,
        PhaseReasonCode::ReviewProjectSummary,
        PhaseReasonCode::ReviewBatch,
        PhaseReasonCode::ReviewFinalUnify,
        PhaseReasonCode::UserApprovedPublish,
        PhaseReasonCode::PublishSuccess,
        PhaseReasonCode::PublishFail,
        PhaseReasonCode::PublishConfirmedSuccess,
        PhaseReasonCode::PublishConfirmedFail,
        PhaseReasonCode::PlanRevisionRequested,
        PhaseReasonCode::PhaseBlocked,
    ];

    const ALL_GUARD_BLOCK_KINDS: [GuardBlockKind; 12] = [
        GuardBlockKind::PlanJsonInvalid,
        GuardBlockKind::PlanGrounding,
        GuardBlockKind::PlanSemanticInvalid,
        GuardBlockKind::PlanDesignCritique,
        GuardBlockKind::BatchLocked,
        GuardBlockKind::AuthoringCompletion,
        GuardBlockKind::AuthoringToValidate,
        GuardBlockKind::MissingGoldModels,
        GuardBlockKind::PrecheckFailed,
        GuardBlockKind::ValidateExecutionFailed,
        GuardBlockKind::MissingThreadStep,
        GuardBlockKind::PhaseExecutionError,
    ];

    #[test]
    fn phase_reason_code_as_str_matches_serde() {
        for code in ALL_PHASE_REASON_CODES {
            let serde_name = serde_json::to_value(code)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            assert_eq!(code.as_str(), serde_name, "as_str drift for {:?}", code);
        }
    }

    #[test]
    fn guard_block_kind_as_str_matches_serde() {
        for kind in ALL_GUARD_BLOCK_KINDS {
            let serde_name = serde_json::to_value(kind)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            assert_eq!(kind.as_str(), serde_name, "as_str drift for {:?}", kind);
        }
    }
}
