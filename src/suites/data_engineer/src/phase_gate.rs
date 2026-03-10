use crate::domain_types::GuardBlockKind;

use crate::control_flow::Phase;
use crate::progress_controller::{
    ExecutionMode, ExecutionState, RepairLadderStep, DEFAULT_MAX_STALL_COUNT,
};

pub type PreTurnDirective = react_core::workflow::PreTurnDirective<GuardBlockKind>;

fn guard_reason(signal: &str, ctx: &[(&str, &str)]) -> String {
    use std::fmt::Write;
    let mut buf = format!("guard_block: {signal}");
    for (k, v) in ctx {
        write!(buf, " | {k}={v}").ok();
    }
    buf
}

pub fn evaluate_pre_turn_directive(
    execution_state: &ExecutionState,
    phase: Phase,
    max_replan_backtracks: usize,
) -> PreTurnDirective {
    let phase_state = execution_state.phase_state();
    let repair_state = execution_state.repair_state();

    if matches!(phase_state.mode, ExecutionMode::Mutate)
        && repair_state.stall_count >= DEFAULT_MAX_STALL_COUNT
    {
        return PreTurnDirective::FailFast {
            kind: GuardBlockKind::AuthoringToValidate,
            reason: guard_reason(
                "stall_count_exceeded",
                &[
                    ("stall_count", &repair_state.stall_count.to_string()),
                    ("max_stall_count", &DEFAULT_MAX_STALL_COUNT.to_string()),
                    ("mode", "mutate"),
                ],
            ),
        };
    }

    if phase_state.replan_backtracks >= max_replan_backtracks {
        return PreTurnDirective::FailFast {
            kind: GuardBlockKind::BatchLocked,
            reason: guard_reason(
                "replan_backtrack_limit",
                &[
                    (
                        "replan_backtracks",
                        &phase_state.replan_backtracks.to_string(),
                    ),
                    ("limit", &max_replan_backtracks.to_string()),
                    ("phase", phase.as_str()),
                    (
                        "action",
                        "inspect validate/review errors and apply a targeted fix",
                    ),
                ],
            ),
        };
    }

    let single_target_repair_path = derive_single_target_repair_path(execution_state);
    if repair_state.hard_mutation_repair_mode()
        && single_target_repair_path.is_some()
        && repair_state.ladder_step() == RepairLadderStep::Stop
    {
        let target = single_target_repair_path.as_deref().unwrap_or("(unknown)");
        return PreTurnDirective::FailFast {
            kind: GuardBlockKind::AuthoringToValidate,
            reason: guard_reason(
                "repair_ladder_stop",
                &[
                    ("target", target.trim()),
                    ("attempts", &repair_state.attempt_count().to_string()),
                    ("action", "apply a manual fix and rerun"),
                ],
            ),
        };
    }

    PreTurnDirective::Proceed
}

pub fn patch_impl_intent_unsatisfied(execution_state: &ExecutionState, phase: Phase) -> bool {
    let repair = execution_state.repair_state();
    let Some(intent) = repair.pending_patch_impl.as_ref() else {
        return false;
    };
    intent.phase == phase && repair.mutation_epoch <= intent.entry_mutation_epoch
}

pub fn derive_single_target_repair_path(execution_state: &ExecutionState) -> Option<String> {
    let repair = execution_state.repair_state();
    repair
        .single_target_repair_path()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preturn_gate_prioritizes_mutate_stall_failfast() {
        let mut st = ExecutionState::new();
        st.phase.mode = ExecutionMode::Mutate;
        st.repair.stall_count = 3;
        let d = evaluate_pre_turn_directive(&st, Phase::CleanseAuthor, 3);
        match d {
            PreTurnDirective::FailFast { kind, .. } => {
                assert_eq!(kind, GuardBlockKind::AuthoringToValidate)
            }
            _ => panic!("expected failfast"),
        }
    }

    #[test]
    fn preturn_gate_replan_backtrack_failfast() {
        let mut st = ExecutionState::new();
        st.phase.replan_backtracks = 4;
        let d = evaluate_pre_turn_directive(&st, Phase::ModelPlan, 3);
        match d {
            PreTurnDirective::FailFast { kind, reason } => {
                assert_eq!(kind, GuardBlockKind::BatchLocked);
                assert!(reason.contains("replan_backtrack_limit"));
            }
            _ => panic!("expected failfast"),
        }
    }

    #[test]
    fn preturn_gate_hard_repair_ladder_stop_failfast() {
        let mut st = ExecutionState::new();
        st.repair.repair_mode = crate::progress_controller::RepairModeState::SqlTarget(
            crate::progress_controller::SqlTargetRepairMode {
                target_path: crate::progress_controller::SqlModelPath::parse(
                    "models/staging/stg_orders.sql".to_string(),
                )
                .expect("valid sql model path"),
                core: crate::progress_controller::RepairModeCore {
                    ladder_step: RepairLadderStep::Stop,
                    attempt_count: 7,
                    repair_started_mutation_epoch: None,
                    consecutive_noop_patches: 0,
                },
            },
        );
        let d = evaluate_pre_turn_directive(&st, Phase::CleanseAuthor, 3);
        match d {
            PreTurnDirective::FailFast { kind, reason } => {
                assert_eq!(kind, GuardBlockKind::AuthoringToValidate);
                assert!(reason.contains("repair_ladder_stop"));
                assert!(reason.contains("stg_orders.sql"));
            }
            _ => panic!("expected failfast"),
        }
    }

    #[test]
    fn preturn_gate_hard_repair_ladder_stop_uses_failed_model_fallback() {
        let mut st = ExecutionState::new();
        st.repair.repair_mode = crate::progress_controller::RepairModeState::SqlTarget(
            crate::progress_controller::SqlTargetRepairMode {
                target_path: crate::progress_controller::SqlModelPath::parse(
                    "models/staging/stg_orders.sql".to_string(),
                )
                .expect("valid sql model path"),
                core: crate::progress_controller::RepairModeCore {
                    ladder_step: RepairLadderStep::Stop,
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
        let d = evaluate_pre_turn_directive(&st, Phase::CleanseAuthor, 3);
        match d {
            PreTurnDirective::FailFast { kind, reason } => {
                assert_eq!(kind, GuardBlockKind::AuthoringToValidate);
                assert!(reason.contains("stg_orders.sql"));
            }
            _ => panic!("expected failfast"),
        }
    }

    #[test]
    fn preturn_gate_table_driven_precedence() {
        struct Case {
            name: &'static str,
            setup: fn(&mut ExecutionState),
            expect_fail: bool,
            expect_kind: Option<GuardBlockKind>,
        }
        let cases = vec![
            Case {
                name: "default_proceed",
                setup: |_| {},
                expect_fail: false,
                expect_kind: None,
            },
            Case {
                name: "stall_has_priority_over_replan",
                setup: |st| {
                    st.phase.mode = ExecutionMode::Mutate;
                    st.repair.stall_count = 3;
                    st.phase.replan_backtracks = 10;
                },
                expect_fail: true,
                expect_kind: Some(GuardBlockKind::AuthoringToValidate),
            },
            Case {
                name: "replan_failfast_when_no_stall",
                setup: |st| {
                    st.phase.replan_backtracks = 5;
                },
                expect_fail: true,
                expect_kind: Some(GuardBlockKind::BatchLocked),
            },
        ];

        for c in cases {
            let mut st = ExecutionState::new();
            (c.setup)(&mut st);
            let d = evaluate_pre_turn_directive(&st, Phase::ModelPlan, 3);
            match (c.expect_fail, d) {
                (false, PreTurnDirective::Proceed) => {}
                (true, PreTurnDirective::FailFast { kind, .. }) => {
                    assert_eq!(Some(kind), c.expect_kind, "case={}", c.name);
                }
                _ => panic!("unexpected directive for case={}", c.name),
            }
        }
    }

    #[test]
    fn control_critical_plan_and_review_reason_details_use_typed_constructors() {
        let phase_plan_src = include_str!("phase_plan.rs");
        assert!(
            phase_plan_src.contains("phase_reason_detail::plan_auto_approved("),
            "phase_plan must use typed constructor for plan auto-approval detail"
        );
        assert!(
            !phase_plan_src.contains("PlanAutoApprovedDetail {"),
            "phase_plan must not inline PlanAutoApprovedDetail literals in transition paths"
        );

        let phase_review_src = include_str!("phase_review.rs");
        assert!(
            phase_review_src.contains("phase_reason_detail::review_decision_transition("),
            "phase_review must use typed constructor for review decision transitions"
        );
        assert!(
            !phase_review_src.contains("ReviewDecisionTransitionDetail {"),
            "phase_review must not inline ReviewDecisionTransitionDetail literals in transition paths"
        );
    }

    #[test]
    fn control_reason_detail_plan_and_author_paths_use_typed_constructors() {
        let phase_plan_src = include_str!("phase_plan.rs");
        assert!(
            phase_plan_src.contains("phase_reason_detail::plan_auto_approved("),
            "plan auto-approved transitions must use typed detail constructor"
        );
        assert!(
            !phase_plan_src.contains("\"auto_approved_in_agent_mode\": true"),
            "phase_plan should not inline auto_approved reason_detail JSON"
        );

        let plan_helpers_src = include_str!("plan_review_helpers.rs");
        assert!(
            plan_helpers_src.contains("AuthoringCompleteReasonDetail"),
            "authoring-complete transition detail must use typed constructor"
        );
        assert!(
            plan_helpers_src.contains("PlanPrunedEmptyDetail"),
            "plan-pruned-empty transition detail must use typed constructor"
        );
        assert!(
            plan_helpers_src.contains("PlanSemanticInvalidErrorsDetail"),
            "plan semantic-invalid transition detail must use typed constructor"
        );
    }
}
