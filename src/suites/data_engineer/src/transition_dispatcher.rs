use crate::domain_types::{GuardBlockKind, PhaseReasonCode};
use crate::state_manager::StateError;
use react_core::session::{Observation, ThreadStep, ThreadStore};
use serde_json::Value;

use crate::control_flow::{
    allowed_next_phases, is_replan_backtrack, replan_backtrack_counter_cap, Phase, TransitionIntent,
};
use crate::progress_controller::ExecutionState;
use crate::state_manager;

pub type PhaseDirective =
    react_core::workflow::PhaseDirective<Phase, PhaseReasonCode, GuardBlockKind>;

#[derive(Debug, thiserror::Error)]
pub enum TransitionError {
    #[error("invalid phase transition: from='{from}' to='{to}' reason='{reason}'")]
    InvalidTransition {
        from: String,
        to: String,
        reason: String,
    },
    #[error("state persist failed during transition: {0}")]
    StatePersistFailed(#[from] StateError),
    #[error("failed to append thread step: {0}")]
    AppendStepFailed(String),
    #[error("transition rollback failed (original: {original}, rollback: {rollback})")]
    RollbackFailed { original: String, rollback: String },
}

pub async fn dispatch_phase_transition(
    store: &ThreadStore,
    thread_id: &str,
    agent: Option<String>,
    from_phase: Option<Phase>,
    phase: Phase,
    intent: TransitionIntent,
    reason_code: Option<PhaseReasonCode>,
    reason_detail: Option<Value>,
) -> Result<(), TransitionError> {
    if let Some(from) = from_phase {
        let is_same_phase_annotation = intent == TransitionIntent::Annotation;
        if !is_same_phase_annotation && !allowed_next_phases(from).contains(&phase) {
            return Err(TransitionError::InvalidTransition {
                from: from.as_str().to_string(),
                to: phase.as_str().to_string(),
                reason: reason_code
                    .map(|c| c.as_str().to_string())
                    .unwrap_or_else(|| "none".to_string()),
            });
        }
    }

    let mut st = state_manager::load_execution_state_strict(&store.control_store(), thread_id)
        .await?
        .unwrap_or_else(ExecutionState::new);
    let prev_state = st.clone();
    if let Some(from) = from_phase {
        let is_backtrack = is_replan_backtrack(from, phase);
        match intent {
            TransitionIntent::Annotation => {}
            TransitionIntent::Forward => {
                st.with_phase_state_mut(|phase_state| {
                    phase_state.replan_backtracks = 0;
                });
            }
            TransitionIntent::Loopback => {
                let current = st.phase_state().replan_backtracks;
                st.with_phase_state_mut(|phase_state| {
                    phase_state.replan_backtracks = react_core::workflow::next_replan_backtracks(
                        current,
                        intent,
                        is_backtrack,
                        replan_backtrack_counter_cap(),
                    );
                });
            }
        }
    }
    if phase == Phase::ModelPlan && from_phase != Some(Phase::ModelPlan) {
        st.reset_manifest_lookup_state();
    }
    if matches!(phase, Phase::CleansePlan | Phase::ModelPlan) && from_phase != Some(phase) {
        st.reset_plan_bootstrap(phase);
    }
    st.with_phase_state_mut(|phase_state| {
        phase_state.current_phase = Some(phase);
        phase_state.phase_reason_code = reason_code.clone();
        phase_state.phase_reason_detail = reason_detail.clone();
    });
    state_manager::replace_execution_state(&store.control_store(), thread_id, st).await?;

    let agent = agent.unwrap_or_else(|| crate::env_util::DEFAULT_AGENT_NAME.to_string());
    if let Err(append_err) = store
        .append_step(
            thread_id,
            ThreadStep::Phase {
                phase: phase.as_str().to_string(),
                from_phase: from_phase.map(|p| p.as_str().to_string()),
                reason_code: reason_code.map(|rc| rc.as_str().to_string()),
                reason_detail,
                observation: Observation::ok(),
                ts: chrono::Utc::now().to_rfc3339(),
                agent,
            },
        )
        .await
    {
        let original = append_err.to_string();
        if let Err(rollback_err) =
            state_manager::replace_execution_state(&store.control_store(), thread_id, prev_state)
                .await
        {
            return Err(TransitionError::RollbackFailed {
                original,
                rollback: rollback_err.to_string(),
            });
        }
        return Err(TransitionError::AppendStepFailed(original));
    }

    Ok(())
}

pub async fn apply_phase_directive(
    store: &ThreadStore,
    thread_id: &str,
    agent: Option<String>,
    from_phase: Option<Phase>,
    directive: PhaseDirective,
) -> Result<(), TransitionError> {
    match directive {
        PhaseDirective::Transition {
            to,
            intent,
            reason_code,
            reason_detail,
        } => {
            dispatch_phase_transition(
                store,
                thread_id,
                agent,
                from_phase,
                to,
                intent,
                reason_code,
                reason_detail,
            )
            .await
        }
        PhaseDirective::Block {
            phase,
            kind,
            reason,
        } => store
            .append_step(
                thread_id,
                ThreadStep::GuardBlock {
                    phase: phase.as_str().to_string(),
                    kind: kind.as_str().to_string(),
                    reason: reason.clone(),
                    observation: Observation::fail(vec![reason]),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: agent.unwrap_or_else(|| crate::env_util::DEFAULT_AGENT_NAME.to_string()),
                },
            )
            .await
            .map_err(|e| TransitionError::AppendStepFailed(e.to_string())),
    }
}

// TODO(item-102): Some guardrail tests below overlap with tests_mod.rs (e.g. source-scanning
// tests like `all_phase_executors_use_phase_contract_transition_seam` and
// `run_agent_source_enforces_kernel_transition_and_guard_paths`). Consolidate into a single
// location to avoid drift.
#[cfg(test)]
mod tests {
    use super::*;
    use react_core::keyspace::DefaultKeyspace;
    use react_core::scope::RequestScope;
    use react_module_storage_memory::InMemoryStorageAdapter;
    use std::sync::Arc;

    #[tokio::test]
    async fn validate_pass_to_review_resets_counter() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let store = ThreadStore::new(storage, scope, keyspace);
        let tid = "tid-validate-pass-reset";

        let mut st = ExecutionState::new();
        st.phase.replan_backtracks = 2;
        state_manager::replace_execution_state(&store.control_store(), tid, st)
            .await
            .expect("seed execution state");

        dispatch_phase_transition(
            &store,
            tid,
            Some("agent".to_string()),
            Some(Phase::CleanseValidate),
            Phase::CleanseReview,
            TransitionIntent::Forward,
            Some(PhaseReasonCode::ValidatePassToReview),
            None,
        )
        .await
        .expect("transition should succeed");

        let got = state_manager::load_execution_state(&store.control_store(), tid)
            .await
            .expect("state should load")
            .expect("state should exist");
        assert_eq!(
            got.phase.replan_backtracks, 0,
            "forward transitions must reset loopback counter"
        );
    }

    #[tokio::test]
    async fn validate_pass_to_author_increments_loopback_counter() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let store = ThreadStore::new(storage, scope, keyspace);
        let tid = "tid-validate-pass-loopback";

        let mut st = ExecutionState::new();
        st.phase.replan_backtracks = 0;
        state_manager::replace_execution_state(&store.control_store(), tid, st)
            .await
            .expect("seed execution state");

        dispatch_phase_transition(
            &store,
            tid,
            Some("agent".to_string()),
            Some(Phase::CleanseValidate),
            Phase::CleanseAuthor,
            TransitionIntent::Loopback,
            Some(PhaseReasonCode::ValidatePassToAuthoring),
            None,
        )
        .await
        .expect("transition should succeed");

        let got = state_manager::load_execution_state(&store.control_store(), tid)
            .await
            .expect("state should load")
            .expect("state should exist");
        assert_eq!(got.phase.replan_backtracks, 1);
    }

    #[tokio::test]
    async fn entering_model_plan_resets_manifest_and_bootstrap_state() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let store = ThreadStore::new(storage, scope, keyspace);
        let tid = "tid-model-plan-reset-state";

        let mut st = ExecutionState::new();
        st.manifest.plan_bootstrap.model_done = true;
        st.manifest.manifest_lookup.retry_suppressed = true;
        st.manifest.manifest_lookup.repeated_failure_count = 3;
        st.manifest.manifest_lookup.failure_signature = Some("NoSuchKey:Ambiguous".to_string());
        state_manager::replace_execution_state(&store.control_store(), tid, st)
            .await
            .expect("seed execution state");

        dispatch_phase_transition(
            &store,
            tid,
            Some("agent".to_string()),
            Some(Phase::ModelReview),
            Phase::ModelPlan,
            TransitionIntent::Loopback,
            Some(PhaseReasonCode::ReviewPatchImpl),
            None,
        )
        .await
        .expect("transition should succeed");

        let got = state_manager::load_execution_state(&store.control_store(), tid)
            .await
            .expect("state should load")
            .expect("state should exist");
        assert!(
            !got.manifest.plan_bootstrap.model_done,
            "model-plan bootstrap should reset on fresh model_plan entry"
        );
        assert!(
            !got.manifest.manifest_lookup.retry_suppressed
                && got.manifest.manifest_lookup.repeated_failure_count == 0,
            "manifest lookup retry state should reset on fresh model_plan entry"
        );
    }

    #[tokio::test]
    async fn apply_phase_directive_block_appends_guard() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let store = ThreadStore::new(storage, scope, keyspace);
        let tid = "tid-phase-directive-block";

        apply_phase_directive(
            &store,
            tid,
            Some("agent".to_string()),
            Some(Phase::CleanseAuthor),
            PhaseDirective::Block {
                phase: Phase::CleanseAuthor,
                kind: GuardBlockKind::AuthoringToValidate,
                reason: "blocked".to_string(),
            },
        )
        .await
        .expect("directive block should append");

        let log = store.get(tid).await.expect("thread log should exist");
        let steps = log.steps;
        assert!(
            steps.iter().any(
                |s| matches!(s, ThreadStep::GuardBlock { phase, .. } if phase == "cleanse_author")
            ),
            "guard block must be appended by directive applier"
        );
    }

    #[tokio::test]
    async fn apply_phase_directive_annotate_keeps_backtrack_counter() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let store = ThreadStore::new(storage, scope, keyspace);
        let tid = "tid-phase-directive-annotate";
        let mut st = ExecutionState::new();
        st.phase.current_phase = Some(Phase::CleansePlan);
        st.phase.replan_backtracks = 2;
        state_manager::replace_execution_state(&store.control_store(), tid, st)
            .await
            .expect("seed state");

        apply_phase_directive(
            &store,
            tid,
            Some("agent".to_string()),
            Some(Phase::CleansePlan),
            PhaseDirective::Transition {
                to: Phase::CleansePlan,
                intent: TransitionIntent::Annotation,
                reason_code: Some(PhaseReasonCode::PhaseSet),
                reason_detail: Some(serde_json::json!({"note":"x"})),
            },
        )
        .await
        .expect("annotation should succeed");

        let got = state_manager::load_execution_state(&store.control_store(), tid)
            .await
            .expect("state")
            .expect("state should exist");
        assert_eq!(got.phase.replan_backtracks, 2);
    }

    #[tokio::test]
    async fn loopback_counter_saturates_at_cap() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let store = ThreadStore::new(storage, scope, keyspace);
        let tid = "tid-loopback-cap";
        let mut st = ExecutionState::new();
        st.phase.current_phase = Some(Phase::CleanseValidate);
        st.phase.replan_backtracks = replan_backtrack_counter_cap();
        state_manager::replace_execution_state(&store.control_store(), tid, st)
            .await
            .expect("seed state");

        dispatch_phase_transition(
            &store,
            tid,
            Some("agent".to_string()),
            Some(Phase::CleanseValidate),
            Phase::CleanseAuthor,
            TransitionIntent::Loopback,
            Some(PhaseReasonCode::ValidatePassToAuthoring),
            None,
        )
        .await
        .expect("transition should succeed");

        let got = state_manager::load_execution_state(&store.control_store(), tid)
            .await
            .expect("state")
            .expect("state should exist");
        assert_eq!(got.phase.replan_backtracks, replan_backtrack_counter_cap());
    }

    #[tokio::test]
    async fn apply_phase_directive_transition_is_handled() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let store = ThreadStore::new(storage, scope, keyspace);
        let tid = "tid-phase-directive-transition";

        apply_phase_directive(
            &store,
            tid,
            Some("agent".to_string()),
            Some(Phase::Preflight),
            PhaseDirective::Transition {
                to: Phase::CleansePlan,
                intent: TransitionIntent::Forward,
                reason_code: Some(PhaseReasonCode::PreflightOk),
                reason_detail: None,
            },
        )
        .await
        .expect("transition should succeed");

        let log = store.get(tid).await.expect("thread log should exist");
        assert!(
            log.steps
                .iter()
                .any(|s| matches!(s, ThreadStep::Phase { phase, .. } if phase == "cleanse_plan")),
            "transition directive must append a phase step"
        );
    }

    #[tokio::test]
    async fn preturn_ladder_stop_fallback_can_be_applied_as_block_directive() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let store = ThreadStore::new(storage, scope, keyspace);
        let tid = "tid-preturn-ladder-stop-fallback";

        let mut st = ExecutionState::new();
        st.repair.repair_mode = crate::progress_controller::RepairModeState::Active(
            crate::progress_controller::RepairMode {
                repair_type: crate::progress_controller::RepairType::SqlTarget,
                target_path: Some(crate::progress_controller::RepairTargetPath::SqlModel(
                    crate::progress_controller::SqlModelPath::parse(
                        "models/staging/stg_orders.sql".to_string(),
                    )
                    .expect("valid sql model path"),
                )),
                materialization: crate::progress_controller::RepairTargetMaterialization::Existing,
                core: crate::progress_controller::RepairModeCore {
                    ladder_step: crate::progress_controller::RepairLadderStep::Stop,
                    attempt_count: 3,
                    repair_started_mutation_epoch: None,
                    consecutive_noop_patches: 0,
                },
            },
        );
        st.telemetry.last_validate = Some(crate::progress_controller::LastValidateState {
            failed_models: vec![crate::progress_controller::FailedModelRef {
                name: "stg_orders".to_string(),
                file: "models/staging/stg_orders.sql".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        });
        let directive =
            crate::phase_gate::evaluate_pre_turn_directive(&st, Phase::CleanseAuthor, 3);
        let crate::phase_gate::PreTurnDirective::FailFast { kind, reason } = directive else {
            panic!("expected failfast directive");
        };
        apply_phase_directive(
            &store,
            tid,
            Some("agent".to_string()),
            Some(Phase::CleanseAuthor),
            PhaseDirective::Block {
                phase: Phase::CleanseAuthor,
                kind,
                reason: reason.clone(),
            },
        )
        .await
        .expect("block directive should append guard");

        let log = store.get(tid).await.expect("thread log should exist");
        assert!(
            log.steps.iter().any(|s| matches!(
                s,
                ThreadStep::GuardBlock { reason: r, .. } if r.contains("stg_orders.sql")
            )),
            "expected guard block with failed-model fallback target in reason"
        );
    }

    // -----------------------------------------------------------------------
    // Architectural guardrail tests
    //
    // These tests use `include_str!` to statically inspect sibling source
    // files and assert structural invariants (e.g. "phase_review must use
    // typed reason_detail constructors, not inline struct literals").
    //
    // They are co-located here because they guard the transition dispatcher's
    // callers and are run as part of the normal `cargo test` suite. Moving
    // them to a separate crate-level integration module would lose the
    // locality benefit without a meaningful architectural win.
    // -----------------------------------------------------------------------

    #[test]
    fn control_reason_detail_review_and_publish_paths_use_typed_constructors() {
        let phase_review_src = include_str!("phase_review.rs");
        assert!(
            phase_review_src.contains("phase_reason_detail::review_decision_transition("),
            "review transitions must use typed reason_detail constructor"
        );
        assert!(
            !phase_review_src.contains("ReviewDecisionTransitionDetail {"),
            "phase_review should not inline review transition reason_detail struct literal"
        );

        let phase_publish_src = include_str!("phase_publish.rs");
        for marker in [
            "phase_reason_detail::publish_approval_state(",
            "phase_reason_detail::publish_observation(",
            "phase_reason_detail::publish_auto_approved(",
            "phase_reason_detail::publish_failure(",
        ] {
            assert!(
                phase_publish_src.contains(marker),
                "phase_publish missing typed reason_detail constructor marker: {marker}"
            );
        }
        assert!(
            !phase_publish_src.contains("PublishFailureDetail {"),
            "phase_publish should not inline publish failure reason_detail struct literal"
        );
    }

    #[test]
    fn control_critical_publish_reason_details_use_typed_constructors() {
        let src = include_str!("phase_publish.rs");
        assert!(
            src.contains("phase_reason_detail::publish_approval_state("),
            "publish approval transition must use typed approval-state constructor"
        );
        assert!(
            src.contains("phase_reason_detail::publish_observation("),
            "publish success transitions must use typed observation constructor"
        );
        assert!(
            src.contains("phase_reason_detail::publish_auto_approved("),
            "publish await-approval loop must use typed auto-approved constructor"
        );
        assert!(
            src.contains("phase_reason_detail::publish_failure("),
            "publish failure transitions must use typed failure constructor"
        );
        assert!(
            !src.contains("PublishApprovalStateDetail {"),
            "phase_publish must not inline PublishApprovalStateDetail literals in transition paths"
        );
        assert!(
            !src.contains("PublishObservationDetail {"),
            "phase_publish must not inline PublishObservationDetail literals in transition paths"
        );
        assert!(
            !src.contains("PublishAutoApprovedDetail {"),
            "phase_publish must not inline PublishAutoApprovedDetail literals in transition paths"
        );
        assert!(
            !src.contains("PublishFailureDetail {"),
            "phase_publish must not inline PublishFailureDetail literals in transition paths"
        );
    }

    #[test]
    fn control_critical_author_reason_details_use_typed_constructors() {
        let src = include_str!("phase_author.rs");
        assert!(
            src.contains("phase_reason_detail::plan_missing("),
            "authoring plan-missing loopback must use typed constructor"
        );
        assert!(
            src.contains("phase_reason_detail::plan_not_approved("),
            "authoring plan-not-approved loopback must use typed constructor"
        );
        assert!(
            src.contains("commit_plan_revision_loopback("),
            "authoring semantic-invalid paths must use commit_plan_revision_loopback"
        );
        assert!(
            src.contains("phase_reason_detail::plan_key("),
            "authoring->validate transitions must use typed plan-key constructor"
        );
        assert!(
            !src.contains("PlanMissingDetail {"),
            "phase_author must not inline PlanMissingDetail literals in transition paths"
        );
        assert!(
            !src.contains("PlanNotApprovedDetail {"),
            "phase_author must not inline PlanNotApprovedDetail literals in transition paths"
        );
        assert!(
            !src.contains("PlanSemanticInvalidDetail {"),
            "phase_author must not inline PlanSemanticInvalidDetail literals in transition paths"
        );
        assert!(
            !src.contains("PlanKeyDetail {"),
            "phase_author must not inline PlanKeyDetail literals in transition paths"
        );
    }

    #[test]
    fn publish_await_phase_is_side_effect_free() {
        let src = include_str!("phase_publish.rs");
        let Some(await_start) =
            src.find("pub(super) async fn execute_publish_await_approval_phase")
        else {
            panic!("phase_publish missing execute_publish_await_approval_phase");
        };
        let Some(publish_start) = src.find("pub(super) async fn execute_publish_phase") else {
            panic!("phase_publish missing execute_publish_phase");
        };
        let await_section = &src[await_start..publish_start];
        assert!(
            !await_section.contains("call_and_record_tool("),
            "publish_await_approval must not invoke publish side-effect tools"
        );
        assert!(
            await_section.contains("gate_publish_progress"),
            "publish_await_approval must enforce explicit approval gate"
        );
    }

    #[test]
    fn review_unknown_tier_routing_is_phase_aware() {
        let src = include_str!("phase_review.rs");
        assert!(
            src.contains("effective_review_tier"),
            "phase_review should use phase-aware review tier normalization"
        );
        assert!(
            src.contains("patch_impl_target_phase"),
            "phase_review should route patch-impl through centralized helper"
        );
        assert!(
            src.contains("effective_review_tier(phase, tier)"),
            "unknown review tier should normalize through phase-aware helper"
        );
    }

    #[test]
    fn preflight_uses_phase_contract_decision_commit() {
        let src = include_str!("phase_preflight.rs");
        assert!(
            src.contains("commit_phase_decision"),
            "phase_preflight should commit transitions through the shared phase contract helper"
        );
        assert!(
            src.contains("PhaseDecision::forward"),
            "phase_preflight should use shared phase decision constructors"
        );
    }

    #[test]
    fn validate_and_author_use_phase_contract_transition_seam() {
        let validate_src = include_str!("phase_validate.rs");
        assert!(
            validate_src.contains("commit_phase_decision"),
            "phase_validate should commit transitions through phase contract helpers"
        );
        assert!(
            !validate_src.contains("apply_phase_transition("),
            "phase_validate should not bypass commit_phase_decision"
        );
        assert!(
            validate_src.contains("reduce_validate_pass_plan_state"),
            "phase_validate should route validate-pass through one reducer"
        );
        assert!(
            validate_src.contains("reduce_validate_pass_plan_state(&actx, phase)"),
            "phase_validate should call the validate-pass reducer before committing transition"
        );
        assert!(
            validate_src.contains("commit_validate_pass_transition"),
            "phase_validate should commit validate-pass transitions through one helper"
        );
        assert!(
            validate_src.contains(".run_observed("),
            "phase_validate must use ThreadStore::run_observed for tool step logging"
        );
        assert!(
            !validate_src.contains("ThreadStep::ToolStart {"),
            "phase_validate must not append raw ToolStart steps directly; use ThreadStore::run_observed"
        );
        assert!(
            !validate_src.contains("ThreadStep::ToolEnd {"),
            "phase_validate must not append raw ToolEnd steps directly; use ThreadStore::run_observed"
        );
        assert!(
            !validate_src.contains(&format!("{}{}", "thread_store", ".get(thread_id)")),
            "phase_validate must not derive runtime control metadata from thread log replay"
        );

        let author_src = include_str!("phase_author.rs");
        assert!(
            author_src.contains("commit_phase_decision"),
            "phase_author should commit transitions through phase contract helpers"
        );
        assert!(
            !author_src.contains("apply_phase_transition("),
            "phase_author should not bypass commit_phase_decision"
        );
        assert!(
            author_src.contains("decide_author_validate_trigger"),
            "phase_author should use a single author->validate decision helper"
        );

        let review_src = include_str!("phase_review.rs");
        assert!(
            !review_src.contains(&format!("{}{}", "thread_store", ".get(thread_id)")),
            "phase_review must not derive runtime control metadata from thread log replay"
        );
    }

    #[test]
    fn all_phase_executors_use_phase_contract_transition_seam() {
        let plan_src = include_str!("phase_plan.rs");
        let review_src = include_str!("phase_review.rs");
        let publish_src = include_str!("phase_publish.rs");
        let helpers_src = include_str!("plan_review_helpers.rs");

        for (name, src) in [
            ("phase_plan", plan_src),
            ("phase_review", review_src),
            ("phase_publish", publish_src),
            ("plan_review_helpers", helpers_src),
        ] {
            assert!(
                src.contains("commit_phase_decision"),
                "{name} should commit transitions through commit_phase_decision"
            );
            assert!(
                !src.contains("apply_phase_transition("),
                "{name} should not bypass commit_phase_decision"
            );
        }
    }

    #[test]
    fn tool_observability_uses_core_run_observed() {
        let control_flow_src = include_str!("control_flow.rs");
        assert!(
            control_flow_src.contains(".run_observed("),
            "call_and_record_tool must delegate to ThreadStore::run_observed"
        );
        assert!(
            !control_flow_src.contains("ThreadStep::ToolStart {"),
            "control_flow must not construct raw ToolStart; use ThreadStore::run_observed"
        );
        assert!(
            !control_flow_src.contains("ThreadStep::ToolEnd {"),
            "control_flow must not construct raw ToolEnd; use ThreadStore::run_observed"
        );

        let policy_src = include_str!("policy_sql_validated.rs");
        assert!(
            policy_src.contains(".run_observed("),
            "policy_sql_validated must delegate to ThreadStore::run_observed for run_sql"
        );
    }

    #[test]
    fn repair_state_hard_cutover_uses_typed_mode_only() {
        let src = include_str!("progress_controller.rs");
        assert!(
            src.contains("enum RepairModeState"),
            "progress_controller should model repair mode as a typed enum"
        );
        assert!(
            src.contains("pub repair_mode: RepairModeState"),
            "execution/repair state should store typed repair_mode"
        );
        assert!(
            !src.contains("pub hard_mutation_repair_mode: bool"),
            "legacy hard_mutation_repair_mode bool field must not exist"
        );
        assert!(
            !src.contains("fn set_active_repair_mode("),
            "temporary set_active_repair_mode helper must not exist after hard cutover"
        );
        assert!(
            !src.contains("fn set_active_repair_mode_snapshot("),
            "temporary set_active_repair_mode_snapshot helper must not exist after hard cutover"
        );
        assert!(
            !src.contains("fn reset_repair_attempts("),
            "temporary reset_repair_attempts helper must not exist after hard cutover"
        );
    }
}
