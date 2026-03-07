use super::*;
use crate::data_engineer::control_flow::Phase;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrecheckAuthoringHandoff {
    None,
    SchemaRepair,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthorValidateTrigger {
    WorkGroupValidate,
    PlanTasksDone,
}

fn precheck_authoring_handoff(reason_code: Option<PhaseReasonCode>) -> PrecheckAuthoringHandoff {
    if reason_code == Some(PhaseReasonCode::PrecheckFailed) {
        PrecheckAuthoringHandoff::SchemaRepair
    } else {
        PrecheckAuthoringHandoff::None
    }
}

fn decide_author_validate_trigger(
    next_action: &crate::data_engineer::plan::AuthoringNextAction,
    completion_snapshot: &crate::data_engineer::plan::PlanCompletionSnapshot,
    last_validate_failed: bool,
) -> Option<AuthorValidateTrigger> {
    if last_validate_failed {
        return None;
    }
    if matches!(
        next_action,
        crate::data_engineer::plan::AuthoringNextAction::Validate
    ) {
        return Some(AuthorValidateTrigger::WorkGroupValidate);
    }
    if completion_snapshot.completion_state() == crate::data_engineer::plan::PlanCompletionState::Complete {
        return Some(AuthorValidateTrigger::PlanTasksDone);
    }
    None
}

async fn transition_plan_missing(
    thread_store: &ThreadStore,
    thread_id: &str,
    phase: Phase,
    track: TrackKind,
) -> Result<(), String> {
    crate::data_engineer::phase_contract::commit_phase_decision(
        thread_store,
        thread_id,
        Some(phase),
        crate::data_engineer::phase_contract::PhaseDecision::loopback(
            track.plan_phase(),
            Some(PhaseReasonCode::PlanMissing),
            Some(crate::data_engineer::phase_reason_detail::plan_missing(
                track,
                format!(
                    "authoring entered without an active {} plan; routing back to planning",
                    track.as_str()
                ),
            )),
        ),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_engineer::control_flow::Phase;

    #[test]
    fn patch_based_repair_step_detection_is_explicit() {
        assert!(is_patch_based_repair_step(
            &crate::data_engineer::progress_controller::RepairLadderStep::PatchTarget
        ));
        assert!(is_patch_based_repair_step(
            &crate::data_engineer::progress_controller::RepairLadderStep::ReplaceContents
        ));
        assert!(!is_patch_based_repair_step(
            &crate::data_engineer::progress_controller::RepairLadderStep::FsOp
        ));
        assert!(!is_patch_based_repair_step(
            &crate::data_engineer::progress_controller::RepairLadderStep::Stop
        ));
    }

    #[test]
    fn missing_target_abort_reason_includes_structured_context() {
        let mut st = crate::data_engineer::progress_controller::ExecutionState::new();
        st.repair.repair_mode =
            crate::data_engineer::progress_controller::RepairModeState::SqlTarget(
                crate::data_engineer::progress_controller::SqlTargetRepairMode {
                    target_path: crate::data_engineer::progress_controller::SqlModelPath::parse(
                        "models/staging/stg_test_raw_raw_order_items.sql".to_string(),
                    )
                    .expect("valid sql model path"),
                    core: crate::data_engineer::progress_controller::RepairModeCore {
                        ladder_step: crate::data_engineer::progress_controller::RepairLadderStep::PatchTarget,
                        attempt_count: 2,
                        repair_started_mutation_epoch: None,
                        consecutive_noop_patches: 0,
                    },
                },
            );
        let repair_ctx = crate::data_engineer::progress_controller::RepairPromptContext {
            brief: Some("compile failed".to_string()),
            ..Default::default()
        };
        let reason = build_missing_target_repair_abort_reason(
            Phase::CleanseAuthor,
            "models/staging/stg_test_raw_raw_order_items.sql",
            Some(
                "picnic/dev/example2/dbt/models/staging/stg_test_raw_raw_order_items.sql"
                    .to_string(),
            ),
            &crate::data_engineer::progress_controller::RepairLadderStep::ReplaceContents,
            &st,
            &repair_ctx,
            Some("not found".to_string()),
        );
        assert!(reason.contains("repair_target_content_unavailable"));
        assert!(reason.contains("stg_test_raw_raw_order_items.sql"));
        assert!(reason.contains("\"ladder_step\": \"ReplaceContents\""));
        assert!(reason.contains("\"storage_read_error\": \"not found\""));
    }

    #[test]
    fn precheck_handoff_is_typed() {
        assert_eq!(
            precheck_authoring_handoff(Some(PhaseReasonCode::PrecheckFailed)),
            PrecheckAuthoringHandoff::SchemaRepair
        );
        assert_eq!(
            precheck_authoring_handoff(Some(PhaseReasonCode::ValidateFail)),
            PrecheckAuthoringHandoff::None
        );
        assert_eq!(
            precheck_authoring_handoff(None),
            PrecheckAuthoringHandoff::None
        );
    }

    #[test]
    fn author_validate_trigger_is_single_source() {
        let incomplete = crate::data_engineer::plan::PlanCompletionSnapshot {
            all_done: false,
            pending_count: 1,
            pending_refs: vec![],
        };
        let complete = crate::data_engineer::plan::PlanCompletionSnapshot {
            all_done: true,
            pending_count: 0,
            pending_refs: vec![],
        };
        assert_eq!(
            decide_author_validate_trigger(
                &crate::data_engineer::plan::AuthoringNextAction::Validate,
                &incomplete,
                true
            ),
            None
        );
        assert_eq!(
            decide_author_validate_trigger(
                &crate::data_engineer::plan::AuthoringNextAction::Validate,
                &incomplete,
                false
            ),
            Some(AuthorValidateTrigger::WorkGroupValidate)
        );
        assert_eq!(
            decide_author_validate_trigger(
                &crate::data_engineer::plan::AuthoringNextAction::None,
                &complete,
                false
            ),
            Some(AuthorValidateTrigger::PlanTasksDone)
        );
        assert_eq!(
            decide_author_validate_trigger(
                &crate::data_engineer::plan::AuthoringNextAction::AuthorSql(vec!["x".to_string()]),
                &incomplete,
                false
            ),
            None
        );
    }
}

async fn transition_plan_not_approved(
    thread_store: &ThreadStore,
    thread_id: &str,
    phase: Phase,
    track: TrackKind,
    status: crate::data_engineer::plan_types::PlanStatus,
) -> Result<(), String> {
    crate::data_engineer::phase_contract::commit_phase_decision(
        thread_store,
        thread_id,
        Some(phase),
        crate::data_engineer::phase_contract::PhaseDecision::loopback(
            track.plan_phase(),
            Some(PhaseReasonCode::PlanNotApproved),
            Some(crate::data_engineer::phase_reason_detail::plan_not_approved(
                status,
            )),
        ),
    )
    .await
}

async fn transition_to_track_validate_with_plan_key(
    thread_store: &ThreadStore,
    thread_id: &str,
    phase: Phase,
    track: TrackKind,
    reason_code: PhaseReasonCode,
    plan_key: String,
) -> Result<(), String> {
    crate::data_engineer::phase_contract::commit_phase_decision(
        thread_store,
        thread_id,
        Some(phase),
        crate::data_engineer::phase_contract::PhaseDecision::forward(
            track.validate_phase(),
            Some(reason_code),
            Some(crate::data_engineer::phase_reason_detail::plan_key(plan_key)),
        ),
    )
    .await
}

async fn track_completion_snapshot_all_done(actx: &AgentCtx, track: TrackKind) -> bool {
    if track.is_cleanse() {
        crate::data_engineer::plan::load_cleanse_plan(actx)
            .await
            .map(|p| {
                crate::data_engineer::plan::snapshot_cleanse_completion(&p).completion_state()
                    == crate::data_engineer::plan::PlanCompletionState::Complete
            })
            .unwrap_or(false)
    } else {
        crate::data_engineer::plan::load_model_plan(actx)
            .await
            .map(|p| {
                crate::data_engineer::plan::snapshot_model_completion(&p).completion_state()
                    == crate::data_engineer::plan::PlanCompletionState::Complete
            })
            .unwrap_or(false)
    }
}

fn build_repair_mode_context(
    track: TrackKind,
    plan_key: &str,
    repair_ctx: &crate::data_engineer::progress_controller::RepairPromptContext,
    ladder_step: &crate::data_engineer::progress_controller::RepairLadderStep,
    attempt_count: usize,
    defer_schema_work: bool,
    all_tasks_done_but_validate_failed: bool,
) -> String {
    let kind = track.as_str();
    let next_action_line = match ladder_step {
        crate::data_engineer::progress_controller::RepairLadderStep::PatchTarget => {
            "Next action: call file with op='patch' targeting the failing path.".to_string()
        }
        crate::data_engineer::progress_controller::RepairLadderStep::ReplaceContents => {
            "Next action: replace_contents step is active; rewrite the target file content with a single guarded patch.".to_string()
        }
        crate::data_engineer::progress_controller::RepairLadderStep::FsOp => {
            "Next action: fs_op step is active; use file op='mv' or op='rm' only for filesystem corrections on the target path.".to_string()
        }
        crate::data_engineer::progress_controller::RepairLadderStep::Stop => {
            "Repair ladder is at stop; do not continue autonomous edits without a manual fix.".to_string()
        }
    };
    let mut ctx = if all_tasks_done_but_validate_failed {
        format!(
            "Approved {kind} plan (stored at: {plan_key}).\nAll plan tasks are currently marked done, but the last dbt_validate failed.\nRepair ladder state: step={:?}, attempt_count={}.\n{next_action_line}\n\nRepair targets (fix these DBT files directly; if patching, use Cursor/Aider hunks-only patch_text).\nExample args: {}\n",
            ladder_step,
            attempt_count,
            crate::data_engineer::patch_contract::single_file_patch_good_example_json()
        )
    } else {
        format!(
            "Approved {kind} plan (stored at: {plan_key}).\nThe last dbt_validate failed and a mutating fix is required before any further validation.\nRepair ladder state: step={:?}, attempt_count={}.\n{next_action_line}\n- If using patch: args.path + args.patch_text (Cursor/Aider hunks-only: '@@ ... @@'; no ---/+++ headers).\nExample args: {}\n\n",
            ladder_step,
            attempt_count,
            crate::data_engineer::patch_contract::single_file_patch_good_example_json()
        )
    };
    ctx.push_str(&repair_ctx.format_error_context());
    if defer_schema_work {
        ctx.push_str(
            "\nIMPORTANT: Defer any new checklist expansion or schema contract work until dbt_validate passes.\n",
        );
    }
    ctx
}

fn is_patch_based_repair_step(
    ladder: &crate::data_engineer::progress_controller::RepairLadderStep,
) -> bool {
    matches!(
        ladder,
        crate::data_engineer::progress_controller::RepairLadderStep::PatchTarget
            | crate::data_engineer::progress_controller::RepairLadderStep::ReplaceContents
    )
}

fn build_missing_target_repair_abort_reason(
    phase: crate::data_engineer::control_flow::Phase,
    target_path: &str,
    target_storage_key: Option<String>,
    ladder: &crate::data_engineer::progress_controller::RepairLadderStep,
    execution_state: &crate::data_engineer::progress_controller::ExecutionState,
    repair_ctx: &crate::data_engineer::progress_controller::RepairPromptContext,
    target_read_error: Option<String>,
) -> String {
    let detail = serde_json::json!({
        "reason": "repair_target_content_unavailable",
        "phase": phase.as_str(),
        "target_path": target_path,
        "target_storage_key": target_storage_key,
        "ladder_step": format!("{:?}", ladder),
        "attempt_count": execution_state.attempt_count(),
        "repair_type": format!("{:?}", execution_state.repair_type()),
        "hard_mutation_repair_mode": execution_state.hard_mutation_repair_mode(),
        "last_validate_brief": repair_ctx.brief.clone(),
        "storage_read_error": target_read_error,
        "action": "repair aborted to avoid blind patch generation; provide target content path/state and retry",
    });
    format!(
        "hard repair aborted: target content unavailable for patch-based repair. {}",
        serde_json::to_string_pretty(&detail).unwrap_or_else(|_| detail.to_string())
    )
}

fn resolve_checklist_item_id(actx: &AgentCtx) -> String {
    actx.exec_ctx
        .as_ref()
        .and_then(|c| c.checklist_item_id.as_deref())
        .unwrap_or(crate::data_engineer::plan::CHECKLIST_SCHEMA_CONTRACT)
        .trim()
        .to_string()
}

fn build_schema_checklist_context(
    track: TrackKind,
    plan_key: &str,
    checklist_item_id: &str,
    ids: &[String],
    expected_paths: &[String],
) -> String {
    let kind = track.as_str();
    let batch_tool = if track.is_cleanse() {
        "apply_next_cleanse_schema_batch"
    } else {
        "apply_next_model_schema_batch"
    };
    let mut ctx = format!(
        "Approved {kind} plan (stored at: {plan_key}).\nPending schema checklist work (checklist_item_id={checklist_item_id} ; max 5):\n- {}\n\nNext action: call {batch_tool} (do NOT call file directly).\n\nExpected model SQL paths:\n- {}\n",
        ids.join("\n- "),
        expected_paths.join("\n- "),
    );
    ctx.push_str("\nIMPORTANT: Do NOT call the SQL batch-authoring tool while schema checklist work remains; continue schema checklist repairs first.\n");
    ctx
}

async fn check_batch_lock_and_loopback(
    thread_store: &ThreadStore,
    thread_id: &str,
    phase: Phase,
    track: TrackKind,
    plan_key: &str,
    consecutive_batch_failures: usize,
    total_batch_failures: usize,
    next_ids: &[String],
    expected_paths: &[String],
) -> Result<Option<PhaseExecutorOutcome>, String> {
    if consecutive_batch_failures
        < crate::data_engineer::controller_kernel::max_consecutive_batch_failures()
    {
        return Ok(None);
    }
    let reason = crate::data_engineer::controller_kernel::build_batch_lock_prompt(
        track,
        plan_key,
        consecutive_batch_failures,
        total_batch_failures,
        next_ids,
        expected_paths,
    );
    apply_guard_block(
        thread_store,
        thread_id,
        phase,
        GuardBlockKind::BatchLocked,
        reason.clone(),
    )
    .await?;
    let label = if track.is_cleanse() { "Cleanse" } else { "Model" };
    let violation = crate::data_engineer::progress_controller::PlanViolation::new(
        phase,
        None,
        format!("{label} batch lock: {reason}"),
    );
    crate::data_engineer::phase_contract::commit_plan_revision_loopback(
        thread_store,
        thread_id,
        phase,
        vec![violation],
        crate::data_engineer::progress_controller::PlanRevisionStrategy::Rewrite,
    )
    .await?;
    Ok(Some(PhaseExecutorOutcome::TransitionCommitted))
}

fn extract_validate_fail_context(
    project_snapshot: &serde_json::Value,
) -> Option<serde_json::Value> {
    project_snapshot
        .as_object()
        .and_then(|obj| obj.get("validate_fail_facts"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.last())
        .cloned()
}

impl DataEngineerSuite {
    pub(super) async fn execute_author_phase(
        thread_store: &ThreadStore,
        thread_id: &str,
        phase: crate::data_engineer::control_flow::Phase,
        question: &str,
        sctx: &SuiteCtx,
        execution_state: &crate::data_engineer::progress_controller::ExecutionState,
        guard: &crate::data_engineer::control_flow::DerivedGuardState,
        _thread_state_step_count: usize,
        repair_ctx: &crate::data_engineer::progress_controller::RepairPromptContext,
    ) -> Result<PhaseExecutorOutcome, String> {

let adapter = crate::data_engineer::authoring_driver::adapter_for_phase(phase)
    .ok_or_else(|| {
        format!(
            "authoring adapter missing for phase '{}'",
            phase.as_str()
        )
    })?;
let is_cleanse =
    adapter.kind() == crate::data_engineer::authoring_driver::AuthoringKind::Cleanse;
let track = if is_cleanse {
    TrackKind::Cleanse
} else {
    TrackKind::Model
};
// Treat precheck-failed handoff as an explicit typed schema-repair entry mode.
let mut phase_guard = guard.clone();
let precheck_handoff = precheck_authoring_handoff(execution_state.phase.phase_reason_code);
if precheck_handoff == PrecheckAuthoringHandoff::SchemaRepair {
    phase_guard.last_validate_failed = true;
    phase_guard.mutated_since_fail = false;
}
let hard_mutation_repair_mode = execution_state.hard_mutation_repair_mode();
let mut repair_type = execution_state.repair_type();
if precheck_handoff == PrecheckAuthoringHandoff::SchemaRepair {
    repair_type = crate::data_engineer::progress_controller::RepairType::Schema;
}
let sys = crate::data_engineer::prompts::with_time_context(if is_cleanse {
    prompts::cleanse_system_prompt()
} else {
    prompts::model_system_prompt()
});
let authoring_ctx = crate::data_engineer::authoring_driver::AuthoringCtx {
    phase,
};
let mut actx = AgentCtx {
    top_k: 30,
    per_step_timeout_secs: 10,
    max_steps: 1,
    thread_id: Some(thread_id.to_string()),
    progress_tx: None,
    pre_step_tx: None,
    trace_tx: sctx.trace_tx.clone(),
    agent_name: Some(crate::data_engineer::env_util::DEFAULT_AGENT_NAME.to_string()),
    policy: std::sync::Arc::new(InterruptOnlyPolicy),
    llm: sctx.llm.clone(),
    storage: sctx.storage.clone(),
    scope: sctx.scope.clone(),
    keyspace: sctx.keyspace.clone(),
    vector: sctx.vector.clone(),
    capabilities: react_core::capability::CapabilityMap::default(),
    thread_store: Some(thread_store.clone()),
    exec_ctx: None,
    resolved_config: sctx.resolved_config.clone(),
};
crate::data_engineer::ctx_ext::copy_capabilities_to_actx(sctx, &mut actx);

// Plan-driven batching: load the approved plan, update progress from the thread log,
// and compute the exact next batch to execute (max 5).
let (plan_context, plan_state): (String, PlanState) =
    if is_cleanse {
        let plan = match crate::data_engineer::plan::load_cleanse_plan_any(
            &actx,
        )
        .await
        {
            Some(p) => p,
            None => {
                // Recovery: authoring was entered, but no plan exists (e.g. restart/resume drift).
                // Bounce back to planning so the thread can rehydrate deterministically.
                transition_plan_missing(&thread_store, thread_id, phase, track).await?;
                return Ok(PhaseExecutorOutcome::TransitionCommitted);
            }
        };
        if let control_flow::AuthoringGate::Block { reason } =
            control_flow::gate_author_phase_execution(&plan)
        {
            let violation = crate::data_engineer::progress_controller::PlanViolation::new(
                phase,
                None,
                format!("Plan is not executable: {reason}"),
            );
            crate::data_engineer::phase_contract::commit_plan_revision_loopback(
                &thread_store, thread_id, phase, vec![violation],
                crate::data_engineer::progress_controller::PlanRevisionStrategy::Rewrite,
            ).await?;
            return Ok(PhaseExecutorOutcome::TransitionCommitted);
        }
        // In deterministic repair mode, the plan is frozen (reference-only).
        // Normal progress is updated at tool-write time; do not reconstruct from thread logs.
        if !hard_mutation_repair_mode {
            let _ =
                crate::data_engineer::plan::save_cleanse_plan(&actx, &plan)
                    .await;
        }

        // Explicit execution context for hierarchical UI (best-effort).
        let next_item = crate::data_engineer::plan::cleanse_next_work_item_ctx(&plan);
        crate::data_engineer::phase_author_lifecycle::bind_execution_context(
            &mut actx,
            track,
            plan.plan_key.clone(),
            next_item,
        );
        {
            let next =
                crate::data_engineer::plan::cleanse_next_authoring_action(&plan)
                    .author_sql_ids();
            let expected_paths = crate::data_engineer::phase_author_lifecycle::collect_expected_paths(
                &plan.tasks,
                &next,
                |task| task.dataset_id.as_str(),
                |task| task.expected_model_path.as_deref(),
            );
            if let Some(outcome) = check_batch_lock_and_loopback(
                &thread_store, thread_id, phase, track,
                &plan.plan_key,
                plan.progress.consecutive_batch_failures,
                plan.progress.total_batch_failures,
                &next, &expected_paths,
            ).await? {
                return Ok(outcome);
            }
        }
        if plan.status != crate::data_engineer::plan::PlanStatus::Approved
            && plan.status != crate::data_engineer::plan::PlanStatus::Completed
        {
            transition_plan_not_approved(
                &thread_store,
                thread_id,
                phase,
                track,
                plan.status,
            )
            .await?;
            return Ok(PhaseExecutorOutcome::TransitionCommitted);
        }
        // Work-group driven selection only (hard cutover).
        let next_action =
            crate::data_engineer::plan::cleanse_next_authoring_action(&plan);
        let next = next_action.author_sql_ids();
        // IMPORTANT: If validation failed and we have not successfully mutated since,
        // the authoring tool registry will be patch-only (hard_mutation_only).
        // In that state, do NOT instruct apply_next_cleanse_batch; force repair-mode guidance.
        if hard_mutation_repair_mode
            && matches!(
                repair_type,
                crate::data_engineer::progress_controller::RepairType::SqlTarget
                    | crate::data_engineer::progress_controller::RepairType::Unknown
            )
        {
            // Repair-first routing: dbt_validate failed for a SQL/runtime-class reason.
            // Even if schema checklist work remains, fix failing SQL targets first.
            let ctx = build_repair_mode_context(
                track,
                &plan.plan_key,
                repair_ctx,
                &execution_state.ladder_step(),
                execution_state.attempt_count(),
                true,
                false,
            );
            (ctx, PlanState::Unconstrained)
        } else if hard_mutation_repair_mode {
            // Schema/precheck failures: prefer schema batch tools when schema checklist work remains.
            if let crate::data_engineer::plan::AuthoringNextAction::AuthorSchema(ids) =
                &next_action
            {
                let checklist_item_id = resolve_checklist_item_id(&actx);
                let expected_paths =
                    crate::data_engineer::phase_author_lifecycle::collect_expected_paths(
                        &plan.tasks,
                        ids,
                        |task| task.dataset_id.as_str(),
                        |task| task.expected_model_path.as_deref(),
                    );
                let ctx = build_schema_checklist_context(
                    track, &plan.plan_key, &checklist_item_id, ids, &expected_paths,
                );
                (ctx, PlanState::CleanseSchemaDatasetIds(ids.clone()))
            } else {
                let mut ctx = format!(
                    "Approved cleanse plan (stored at: {}).\nThe last dbt_validate failed and a mutating fix is required before any further validation.\n\nRepair targets (fix these DBT files directly with file op=patch|rm|mv; if patching, use Cursor/Aider hunks-only patch_text).\nExample args: {}\n\n",
                    plan.plan_key,
                    crate::data_engineer::patch_contract::single_file_patch_good_example_json()
                );
                ctx.push_str(&repair_ctx.format_error_context());
                let plan_state = if !next.is_empty() {
                    PlanState::CleanseSqlDatasetIds(next.clone())
                } else {
                    PlanState::Unconstrained
                };
                (ctx, plan_state)
            }
        } else if next.is_empty() {
            // If work-groups exist, interpret "no next SQL batch" as:
            // - either we're blocked on schema checklist authoring, OR
            // - we're ready to transition to validate.
            if let crate::data_engineer::plan::AuthoringNextAction::AuthorSchema(ids) =
                &next_action
            {
                let checklist_item_id = resolve_checklist_item_id(&actx);
                let expected_paths =
                    crate::data_engineer::phase_author_lifecycle::collect_expected_paths(
                        &plan.tasks,
                        ids,
                        |task| task.dataset_id.as_str(),
                        |task| task.expected_model_path.as_deref(),
                    );
                let ctx = build_schema_checklist_context(
                    track, &plan.plan_key, &checklist_item_id, ids, &expected_paths,
                );
                (
                    ctx,
                    PlanState::CleanseSchemaDatasetIds(ids.clone()),
                )
            } else {
                let completion_snapshot =
                    crate::data_engineer::plan::snapshot_cleanse_completion(&plan);
                if let Some(trigger) = decide_author_validate_trigger(
                    &next_action,
                    &completion_snapshot,
                    phase_guard.last_validate_failed,
                ) {
                    let reason_code = match trigger {
                        AuthorValidateTrigger::WorkGroupValidate => {
                            PhaseReasonCode::WorkGroupValidate
                        }
                        AuthorValidateTrigger::PlanTasksDone => {
                            PhaseReasonCode::PlanTasksDone
                        }
                    };
                    transition_to_track_validate_with_plan_key(
                        &thread_store,
                        thread_id,
                        phase,
                        track,
                        reason_code,
                        plan.plan_key.clone(),
                    )
                    .await?;
                    return Ok(PhaseExecutorOutcome::TransitionCommitted);
                }

                // If validation previously failed, do NOT bounce straight back to validate.
                // Run a repair authoring pass grounded in the failing model/file evidence.
                if guard.last_validate_failed {
                    let ctx = build_repair_mode_context(
                        track,
                        &plan.plan_key,
                        repair_ctx,
                        &execution_state.ladder_step(),
                        execution_state.attempt_count(),
                        false,
                        true,
                    );
                    (
                        ctx,
                        PlanState::Unconstrained,
                    )
                } else {
                    let violation = crate::data_engineer::progress_controller::PlanViolation::new(
                        phase,
                        None,
                        "Approved cleanse plan is not executable: no next work-group action while checklist work remains",
                    );
                    crate::data_engineer::phase_contract::commit_plan_revision_loopback(
                        &thread_store, thread_id, phase, vec![violation],
                        crate::data_engineer::progress_controller::PlanRevisionStrategy::Rewrite,
                    ).await?;
                    return Ok(PhaseExecutorOutcome::TransitionCommitted);
                }
            }
        } else {
            (
            format!(
								"Approved cleanse plan (stored at: {}).\nNext batch (deterministic, max 5):\n- {}\n\nNext action: call the deterministic batch authoring tool from the current tool card (do NOT call staging_model directly).",
            plan.plan_key,
            next.join("\n- ")
            ),
            PlanState::CleanseSqlDatasetIds(next.clone()),
        )
        }
    } else {
        let mut plan = match crate::data_engineer::plan::load_model_plan_any(
            &actx,
        )
        .await
        {
            Some(p) => p,
            None => {
                // Recovery: authoring was entered, but no plan exists (e.g. restart/resume drift).
                // Bounce back to planning so the thread can rehydrate deterministically.
                transition_plan_missing(&thread_store, thread_id, phase, track).await?;
                return Ok(PhaseExecutorOutcome::TransitionCommitted);
            }
        };
        if let control_flow::AuthoringGate::Block { reason } =
            control_flow::gate_author_phase_execution(&plan)
        {
            let violation = crate::data_engineer::progress_controller::PlanViolation::new(
                phase,
                None,
                format!("Plan is not executable: {reason}"),
            );
            crate::data_engineer::phase_contract::commit_plan_revision_loopback(
                &thread_store, thread_id, phase, vec![violation],
                crate::data_engineer::progress_controller::PlanRevisionStrategy::Rewrite,
            ).await?;
            return Ok(PhaseExecutorOutcome::TransitionCommitted);
        }
        // In deterministic repair mode, the plan is frozen (reference-only).
        // Normal progress is updated at tool-write time; do not reconstruct from thread logs.
        if !hard_mutation_repair_mode {
            let _ =
                crate::data_engineer::plan::save_model_plan(&actx, &plan)
                    .await;
        }

        // Explicit execution context for hierarchical UI (best-effort).
        let next_item = crate::data_engineer::plan::model_next_work_item_ctx(&plan);
        crate::data_engineer::phase_author_lifecycle::bind_execution_context(
            &mut actx,
            track,
            plan.plan_key.clone(),
            next_item,
        );
        {
            let next =
                crate::data_engineer::plan::model_next_authoring_action(&plan)
                    .author_sql_ids();
            let expected_paths = crate::data_engineer::phase_author_lifecycle::collect_expected_paths(
                &plan.tasks,
                &next,
                |task| task.name.as_str(),
                |task| task.expected_model_path.as_deref(),
            );
            if let Some(outcome) = check_batch_lock_and_loopback(
                &thread_store, thread_id, phase, track,
                &plan.plan_key,
                plan.progress.consecutive_batch_failures,
                plan.progress.total_batch_failures,
                &next, &expected_paths,
            ).await? {
                return Ok(outcome);
            }
        }
        if plan.status != crate::data_engineer::plan::PlanStatus::Approved
            && plan.status != crate::data_engineer::plan::PlanStatus::Completed
        {
            transition_plan_not_approved(
                &thread_store,
                thread_id,
                phase,
                track,
                plan.status,
            )
            .await?;
            return Ok(PhaseExecutorOutcome::TransitionCommitted);
        }
        // Work-group driven selection only (hard cutover).
        let next_action =
            crate::data_engineer::plan::model_next_authoring_action(&plan);
        let next_names = next_action.author_sql_ids();
        // IMPORTANT: If validation failed and we have not successfully mutated since,
        // the authoring tool registry will be patch-only (hard_mutation_only).
        // In that state, do NOT instruct apply_next_model_batch; force repair-mode guidance.
        if hard_mutation_repair_mode
            && matches!(
                repair_type,
                crate::data_engineer::progress_controller::RepairType::SqlTarget
                    | crate::data_engineer::progress_controller::RepairType::Unknown
            )
        {
            // Repair-first routing: dbt_validate failed for a SQL/runtime-class reason.
            // Even if schema checklist work remains, fix failing SQL targets first.
            let ctx = build_repair_mode_context(
                track,
                &plan.plan_key,
                repair_ctx,
                &execution_state.ladder_step(),
                execution_state.attempt_count(),
                true,
                false,
            );
            (ctx, PlanState::Unconstrained)
        } else if hard_mutation_repair_mode {
            if let crate::data_engineer::plan::AuthoringNextAction::AuthorSchema(ids) =
                &next_action
            {
                let checklist_item_id = resolve_checklist_item_id(&actx);
                let expected_paths =
                    crate::data_engineer::phase_author_lifecycle::collect_expected_paths(
                        &plan.tasks,
                        ids,
                        |task| task.name.as_str(),
                        |task| task.expected_model_path.as_deref(),
                    );
                let ctx = build_schema_checklist_context(
                    track, &plan.plan_key, &checklist_item_id, ids, &expected_paths,
                );
                (
                    ctx,
                    PlanState::ModelSchemaItemNames(ids.clone()),
                )
            } else {
                let mut ctx = format!(
                    "Approved model plan (stored at: {}).\nThe last dbt_validate failed and a mutating fix is required before any further validation.\n\nRepair targets (fix these DBT files directly with file op=patch|rm|mv; if patching, use Cursor/Aider hunks-only patch_text).\nExample args: {}\n\n",
                    plan.plan_key,
                    crate::data_engineer::patch_contract::single_file_patch_good_example_json()
                );
                ctx.push_str(&repair_ctx.format_error_context());
                let plan_state = if !next_names.is_empty() {
                    PlanState::ModelSqlItemNames(next_names.clone())
                } else {
                    PlanState::Unconstrained
                };
                (ctx, plan_state)
            }
        } else if next_names.is_empty() {
            if let crate::data_engineer::plan::AuthoringNextAction::AuthorSchema(ids) =
                &next_action
            {
                // Deterministic pre-check: if models/schema.yml already contains model stanzas
                // for these pending items, mark schema_contract done and re-run planning for the
                // next action instead of thrashing the same file.
                {
                    let key = crate::data_engineer::project_fs::join_storage_key(
                        &actx,
                        crate::data_engineer::project_fs::MODELS_SCHEMA_YML,
                    );
                    if let Ok(bytes) = actx.storage.get_bytes(&key).await {
                        let content =
                            String::from_utf8_lossy(&bytes).to_string();
                        if let Ok(vy) =
                            serde_yaml::from_str::<serde_yaml::Value>(&content)
                        {
                            let mut names_in_schema: std::collections::HashSet<
                                String,
                            > = std::collections::HashSet::new();
                            if let Some(models) =
                                vy.get("models").and_then(|m| m.as_sequence())
                            {
                                for m in models.iter() {
                                    if let Some(nm) = m
                                        .get("name")
                                        .and_then(|n| n.as_str())
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                    {
                                        names_in_schema.insert(nm);
                                    }
                                }
                            }
                            let mut changed = false;
                            let checklist_item_id = resolve_checklist_item_id(&actx);
                            for n in ids.iter() {
                                if !names_in_schema.contains(n) {
                                    return Ok(PhaseExecutorOutcome::StayInPhase);
                                }
                                if let Some(t) =
                                    plan.tasks.iter().find(|t| t.name == *n)
                                {
                                    let done = t
                                        .checklist
                                        .iter()
                                        .find(|it| it.checklist_item_id == checklist_item_id)
                                        .map(|it| {
                                            it.status
                                                == crate::data_engineer::plan::ChecklistItemStatus::Done
                                        })
                                        .unwrap_or(false);
                                    if !done {
                                        changed = true;
                                    }
                                }
                                crate::data_engineer::plan::model_checklist_mark_status(
                                    &mut plan,
                                    n,
                                    &checklist_item_id,
                                    crate::data_engineer::plan::ChecklistItemStatus::Done,
                                );
                            }
                            if changed {
                                crate::data_engineer::plan::save_model_plan(
                                    &actx, &plan,
                                )
                                .await?;
                                return Ok(PhaseExecutorOutcome::StayInPhase);
                            }
                        }
                    }
                }

                let expected_paths =
                    crate::data_engineer::phase_author_lifecycle::collect_expected_paths(
                        &plan.tasks,
                        ids,
                        |task| task.name.as_str(),
                        |task| task.expected_model_path.as_deref(),
                    );
                let checklist_item_id = resolve_checklist_item_id(&actx);
                let ctx = build_schema_checklist_context(
                    track, &plan.plan_key, &checklist_item_id, ids, &expected_paths,
                );
                (ctx, PlanState::Unconstrained)
            } else {
                let completion_snapshot =
                    crate::data_engineer::plan::snapshot_model_completion(&plan);
                if let Some(trigger) = decide_author_validate_trigger(
                    &next_action,
                    &completion_snapshot,
                    phase_guard.last_validate_failed,
                ) {
                    let reason_code = match trigger {
                        AuthorValidateTrigger::WorkGroupValidate => {
                            PhaseReasonCode::WorkGroupValidate
                        }
                        AuthorValidateTrigger::PlanTasksDone => {
                            PhaseReasonCode::PlanTasksDone
                        }
                    };
                    transition_to_track_validate_with_plan_key(
                        &thread_store,
                        thread_id,
                        phase,
                        track,
                        reason_code,
                        plan.plan_key.clone(),
                    )
                    .await?;
                    return Ok(PhaseExecutorOutcome::TransitionCommitted);
                }

                if guard.last_validate_failed {
                    let ctx = build_repair_mode_context(
                        track,
                        &plan.plan_key,
                        repair_ctx,
                        &execution_state.ladder_step(),
                        execution_state.attempt_count(),
                        false,
                        true,
                    );
                    (ctx, PlanState::Unconstrained)
                } else {
                    let violation = crate::data_engineer::progress_controller::PlanViolation::new(
                        phase,
                        None,
                        "Approved model plan is not executable: no next work-group action while checklist work remains",
                    );
                    crate::data_engineer::phase_contract::commit_plan_revision_loopback(
                        &thread_store, thread_id, phase, vec![violation],
                        crate::data_engineer::progress_controller::PlanRevisionStrategy::Rewrite,
                    ).await?;
                    return Ok(PhaseExecutorOutcome::TransitionCommitted);
                }
            }
        } else {
            let allowed =
                PlanState::ModelSqlItemNames(next_names.clone());
            // Include task details for the next batch so the LLM can call gold_model with full args.
            let mut details: Vec<String> = Vec::new();
            for n in next_names.iter() {
                if let Some(t) = plan.tasks.iter().find(|t| t.name == *n) {
                    details.push(format!(
                        "- name: {}\n  folder: {}\n  goal: {}\n  inputs: {:?}",
                        t.name, t.folder, t.goal, t.inputs
                    ));
                } else {
                    details.push(format!("- name: {}", n));
                }
            }
            (
            format!(
								"Approved model plan (stored at: {}).\nNext batch (deterministic, max 5):\n{}\n\nNext action: call the deterministic batch authoring tool from the current tool card (do NOT call gold_model directly).",
            plan.plan_key,
            details.join("\n")
            ),
            allowed,
        )
        }
    };

let single_target_repair_path = if hard_mutation_repair_mode {
    crate::data_engineer::phase_gate::derive_single_target_repair_path(&execution_state)
} else {
    None
};
let (registry, tools_card) = Self::build_tools_for_phase(
    phase,
    &phase_guard,
    false,
    sctx,
    &plan_state,
    single_target_repair_path.clone(),
    false,
)?;

// Ground the next authoring pass with last validation summary (if any) and guard state.
let mut q = if is_cleanse {
    Self::inject_cleanse_question(question)
} else {
    Self::inject_model_question(question)
};
if !is_cleanse {
    // Ground gold authoring with the current silver inventory so the agent
    // can reliably build marts from existing stg_* models (no guessing).
    let base = actx
        .keyspace
        .scoped_prefix(&actx.scope, &["dbt"])
        .trim_end_matches('/')
        .to_string();
    let pref = format!("{}/models/staging/", base);
    if let Ok(keys) = actx.storage.list_prefix(&pref).await {
        let mut rels: Vec<String> = keys
            .into_iter()
            .filter(|k| k.ends_with(".sql") && !k.contains("/_versions/"))
            .filter_map(|k| {
                k.strip_prefix(&(base.clone() + "/")).map(|s| s.to_string())
            })
            .collect();
        rels.sort();
        rels.dedup();
        let mut names: Vec<String> = rels
            .into_iter()
            .filter_map(|rel| {
                std::path::Path::new(&rel)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
            })
            .collect();
        names.sort();
        names.dedup();
        if !names.is_empty() {
            q.push_str("\n\nCurrent staged silver models (use ref('stg_*') from these):\n");
            for n in names.into_iter().take(60) {
                q.push_str("- ");
                q.push_str(&n);
                q.push('\n');
            }
        }
    }
}
q.push_str("\n\nNOTE: In agent mode, validation and publish are handled by the suite phases. Do not call dbt_validate or publish tools; focus on authoring fixes and models.");
q.push_str("\nIMPORTANT: Tool-call argument shapes are strict. In particular: vect_query uses args.query_text (NOT args.query) and scope must be \"dataset\"|\"field\"|\"doc\"|\"artifact\"|\"metric\"|\"model\".");
q.push_str("\nIMPORTANT: sql_stats and sql_sample both require args.field. To sample rows, use run_sql with a LIMIT.");
q.push_str("\nIMPORTANT: This authoring phase is plan-driven. Follow the Plan context below. If it says to patch failing DBT files, do that first; if it provides a next batch, execute it. Do NOT ask for approval; approvals happen in plan phases.");
q.push_str("\nIMPORTANT: No downstream compensation exists for incomplete plan structure. If execution context is incomplete, return to planning; do not invent fallback execution.");
q.push_str("\n\nPlan context:\n");
q.push_str(&plan_context);
// Auto-attach authoritative schema facts (no ambiguity) for this phase.
// - In authoring, include ALL relations in the current approved batch.
// - Also include any recent validate-fail facts snapshot if present.
{
    let dialect = crate::data_engineer::facts::SqlDialect(
        crate::data_engineer::resolved_config_from_ctx(&actx)
            .as_ref()
            .map(|cfg| crate::data_engineer::dbt_repair::remediate::active_provider_dialect(cfg))
            .unwrap_or_else(|| "Unknown SQL dialect".to_string()),
    );
    let mut batch_relations: Vec<String> = Vec::new();
    let mut prior_validate_facts: Option<serde_json::Value> = None;
    if is_cleanse {
        if let Some(p) =
            crate::data_engineer::plan::load_cleanse_plan(&actx).await
        {
            if let PlanState::CleanseSqlDatasetIds(ds)
            | PlanState::CleanseSchemaDatasetIds(ds) = &plan_state
            {
                batch_relations =
                    crate::data_engineer::facts::dataset_ids_to_fqns(ds);
            }
            prior_validate_facts = extract_validate_fail_context(&p.project_snapshot);
        }
    } else {
        if let Some(p) =
            crate::data_engineer::plan::load_model_plan(&actx).await
        {
            if let PlanState::ModelSqlItemNames(names)
            | PlanState::ModelSchemaItemNames(names) = &plan_state
            {
                {
                    // Include relations for the models in the batch AND their declared inputs.
                    let mut want_names: Vec<String> = names.clone();
                    for n in names.iter() {
                        if let Some(t) = p.tasks.iter().find(|t| t.name == *n) {
                            for inp in t.inputs.iter() {
                                let s = inp.trim();
                                if !s.is_empty() {
                                    want_names.push(s.to_string());
                                }
                            }
                        }
                    }
                    want_names.sort();
                    want_names.dedup();
                    batch_relations =
                        crate::data_engineer::facts::resolve_model_names_to_fqns(&actx, &want_names).await;
                }
            }
            prior_validate_facts = extract_validate_fail_context(&p.project_snapshot);
        }
    }

    if !batch_relations.is_empty() {
        let limits = crate::data_engineer::facts::FactsLimits::for_scope(
            crate::data_engineer::facts::FactsScope::AuthorBatch,
        );
        let bundle =
            crate::data_engineer::facts::build_facts_bundle_from_relations(
                &actx,
                crate::data_engineer::facts::FactsScope::AuthorBatch,
                dialect.clone(),
                crate::data_engineer::facts::TargetFacts::default(),
                &batch_relations,
                limits,
            )
            .await;
        q.push_str("\n\nIMMUTABLE FACTS (author_batch_schema):\n");
        q.push_str(
            &serde_json::to_string_pretty(&bundle)
                .unwrap_or_else(|_| "{}".to_string()),
        );
        q.push('\n');
        q.push_str("Rules:\n- You MUST NOT reference any column not present in facts.relations[].columns for that relation.\n- If required facts are missing, call sql_schema and then patch.\n");
    }
    if let Some(vf) = prior_validate_facts {
        q.push_str("\n\nIMMUTABLE FACTS (latest_validate_fail_facts):\n");
        q.push_str(
            &serde_json::to_string_pretty(&vf)
                .unwrap_or_else(|_| "{}".to_string()),
        );
        q.push('\n');
    }
}
if repair_ctx.has_context() {
    q.push_str("\n\n");
    q.push_str(&repair_ctx.format_error_context());
    if !repair_ctx.failed_models.is_empty() {
        q.push_str("Fix these first (prefer patching the listed file paths).\n");
    }
}
if let Some(ref target) = single_target_repair_path {
    q.push_str("\n\nDETERMINISTIC SINGLE-TARGET REPAIR MODE:\n");
    q.push_str("- You MUST mutate ONLY this file path in your next mutation (file op=patch|rm|mv):\n");
    q.push_str("- ");
    q.push_str(target);
    q.push_str("\n- Do NOT patch, move, or remove any other file until this target validates.\n");
}

// When the suite is in hard_mutation_only, file is patch-only (no op=get),
// so we MUST include the raw file content for at least the primary failing target.
if hard_mutation_repair_mode && !repair_ctx.failed_models.is_empty() {
    if let Some(file) = Some(repair_ctx.failed_models[0].file.as_str()) {
        let file = file.trim();
        if !file.is_empty() && file != "(unknown file)" {
            let base = actx
                .keyspace
                .scoped_prefix(&actx.scope, &["dbt"])
                .trim_end_matches('/')
                .to_string();
            let key = format!("{}/{}", base, file);
            if let Ok(bytes) = actx.storage.get_bytes(&key).await {
                let content = String::from_utf8_lossy(&bytes).to_string();
                let fence_lang = if file.ends_with(".yml") || file.ends_with(".yaml") {
                    "yaml"
                } else {
                    "sql"
                };
                q.push_str("\n\nPrimary repair target current file content:\n");
                q.push_str("File: ");
                q.push_str(file);
                q.push_str("\n\n```");
                q.push_str(fence_lang);
                q.push_str("\n");
                q.push_str(&content);
                if !content.ends_with('\n') {
                    q.push('\n');
                }
                q.push_str("```\n");
            }
        }
    }
}
if precheck_handoff == PrecheckAuthoringHandoff::SchemaRepair {
    if let Some(detail) = execution_state.phase.phase_reason_detail.as_ref() {
        q.push_str("\n\nPre-check failure detail (fix before validate):\n");
        q.push_str(
            &serde_json::to_string_pretty(detail).unwrap_or_else(|_| detail.to_string()),
        );
    }
}
// If we re-entered authoring due to review feedback, inject the full review text (by ref)
// so the agent can address it in implementation without reopening the plan.
if execution_state.phase.phase_reason_code == Some(PhaseReasonCode::ReviewPatchImpl) {
    if let Some(key) = execution_state
        .phase
        .phase_reason_detail
        .as_ref()
        .and_then(|v| v.get("meta"))
        .and_then(|v| v.get("review_ref"))
        .and_then(|v| v.get("key"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        if let Ok(bytes) = actx.storage.get_bytes(&key).await {
            let txt = String::from_utf8_lossy(&bytes).to_string();
            if !txt.trim().is_empty() {
                q.push_str("\n\nPRIOR REVIEW FEEDBACK (must address by editing implementation; do NOT change the approved plan/spec):\n");
                q.push_str(txt.trim());
                q.push('\n');
            }
        }
    }
}
if hard_mutation_repair_mode {
    q.push_str("\n\nConstraint: your next steps must APPLY A MUTATING FIX before attempting dbt_validate again.");
}
if phase_guard.probe_required && !phase_guard.probe_satisfied {
    q.push_str("\n\nConstraint: runtime validation failed after compile; run meaningful run_sql probes (not SELECT 1) to diagnose data before re-validating. Multiple probes are allowed while they add new signal; repeated same/no-signal probes require you to switch to a mutating file fix.");
}

// Deterministic invariant: do not allow leaving authoring without any models.
let has_models = control_flow::invariant_has_any_models(&actx)
    .await
    .unwrap_or(false);
if !has_models {
    q.push_str("\n\nIMPORTANT: invariant failed: there are no DBT model SQL files yet. Your first task is to create at least one staging model under models/ using staging_model or file op=patch.");
}

// Hard cutover: deterministic repair-mode mini-context.
// When validate failed and we are in single-target repair mode, do NOT feed the model the full
// accumulated authoring prompt (plan context, immutable facts, review text, etc).
// Instead, provide a minimal packet: target file + failure brief + strict allowed operation.
if hard_mutation_repair_mode
    && matches!(
        repair_type,
        crate::data_engineer::progress_controller::RepairType::SqlTarget
            | crate::data_engineer::progress_controller::RepairType::Unknown
    )
    && single_target_repair_path.as_ref().is_some()
{
    let target = single_target_repair_path
        .as_ref()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let mut es = crate::data_engineer::progress_controller::ExecutionState::load(
        &thread_store,
        thread_id,
    )
    .await
    .unwrap_or_else(
        crate::data_engineer::progress_controller::ExecutionState::new,
    );
    if es.single_target_repair_path().as_deref().unwrap_or("").trim().is_empty()
        && !target.is_empty()
    {
        if let Ok(path) = crate::data_engineer::progress_controller::SqlModelPath::parse(target.clone()) {
            es.ensure_repair_target_path(path);
        }
        es.save(&thread_store, thread_id).await.map_err(|e| {
            format!(
                "failed to persist execution-state target path in deterministic repair mode: {e}"
            )
        })?;
    }
    let ladder = es.ladder_step();

    let mut content = String::new();
    let mut target_exists = false;
    let mut target_storage_key: Option<String> = None;
    let mut target_read_error: Option<String> = None;
    if !target.is_empty() {
    let base = actx
        .keyspace
        .scoped_prefix(&actx.scope, &["dbt"])
        .trim_end_matches('/')
        .to_string();
    let key = format!("{}/{}", base, target);
        target_storage_key = Some(key.clone());
        match actx.storage.get_bytes(&key).await {
            Ok(bytes) => {
                target_exists = true;
                content = String::from_utf8_lossy(&bytes).to_string();
            }
            Err(e) => {
                target_read_error = Some(e.to_string());
            }
        }
    }

    // Hard fallback: patch-based repair without reliable target content causes blind hunk generation.
    // Bubble a structured error immediately instead of looping on context-miss patches.
    if is_patch_based_repair_step(&ladder) && !target_exists {
        let reason = build_missing_target_repair_abort_reason(
            phase,
            &target,
            target_storage_key,
            &ladder,
            &es,
            repair_ctx,
            target_read_error,
        );
        apply_guard_block(
            &thread_store,
            thread_id,
            phase,
            GuardBlockKind::AuthoringToValidate,
            reason.clone(),
        )
        .await?;
        return Err(reason);
    }

    let envelope = crate::data_engineer::prompt_packets::PromptEnvelope {
        phase,
        goal: question.trim().to_string(),
        directive: crate::data_engineer::prompt_packets::TurnDirective::Repair,
        plan: None,
        batch: None,
        repair: Some(crate::data_engineer::prompt_packets::RepairPacket {
            target_path: target.clone(),
            ladder_step: ladder.clone(),
            last_validate_brief: repair_ctx.brief.clone(),
            patch_contract: Some(
                crate::data_engineer::prompts::patch_contract::file_patch_contract()
                    .to_string(),
            ),
        }),
    };
    let mut repair =
        crate::data_engineer::prompt_packets::render_envelope(&envelope)
            .map_err(|e| format!("invalid repair prompt envelope: {e}"))?;

    repair.push_str("\nRules:\n");
    repair.push_str("- You MUST call file with a mutating op next.\n");
    repair.push_str("- You MUST mutate ONLY the target file above.\n");
    repair.push_str("- Do NOT use placeholder patch headers like '@@ ... @@'; use real hunks with exact context from the current file content.\n");
    if target.ends_with(".yml") || target.ends_with(".yaml") {
        repair.push_str("- YAML repair rule: edit existing keys in place; do NOT append duplicate top-level keys like 'version:' or 'models:'.\n");
    }
    match ladder {
        crate::data_engineer::progress_controller::RepairLadderStep::PatchTarget => {
            repair.push_str("- REQUIRED OP MODE: patch_target. Use file op='patch' only.\n");
        }
        crate::data_engineer::progress_controller::RepairLadderStep::ReplaceContents => {
            repair.push_str("- REQUIRED OP MODE: replace_contents. Use file op='patch' only and rewrite target content decisively.\n");
        }
        crate::data_engineer::progress_controller::RepairLadderStep::FsOp => {
            repair.push_str("- REQUIRED OP MODE: fs_op. Use file op='mv' or op='rm' only (no patch in this step).\n");
        }
        crate::data_engineer::progress_controller::RepairLadderStep::Stop => {
            repair.push_str("- STOP: prior repair attempts did not converge. Do not continue.\n");
        }
    }

    let fence_lang = if target.ends_with(".yml") || target.ends_with(".yaml") {
        "yaml"
    } else {
        "sql"
    };
    repair.push_str("\nCurrent target file content:\n```");
    repair.push_str(fence_lang);
    repair.push_str("\n");
    repair.push_str(&content);
    if !content.ends_with('\n') {
        repair.push('\n');
    }
    repair.push_str("```\n");

    let error_ctx = repair_ctx.format_error_context();
    if !error_ctx.is_empty() {
        repair.push_str("\n");
        repair.push_str(&error_ctx);
    }

    q = repair;
}

let llm_options = if is_cleanse {
    let author_max_tokens: u32 = crate::data_engineer::env_util::env_u32(
        crate::data_engineer::env_util::env_keys::LLM_AUTHOR_MAX_TOKENS_CLEANSE,
    )
    .unwrap_or(24_000)
    .max(2_000)
    .min(64_000);
    LlmCallOptions {
        prompt_id: "data_engineer.cleanse_author",
        thread_id: None,
        expected_format: react_core::llm::LlmExpectedFormat::JsonObject,
        temperature: Some(0.05),
        top_p: Some(1.0),
        max_output_tokens: Some(author_max_tokens),
        reasoning_effort: None,
        timeout_secs: None,
    }
} else {
    let author_max_tokens: u32 = crate::data_engineer::env_util::env_u32(
        crate::data_engineer::env_util::env_keys::LLM_AUTHOR_MAX_TOKENS_MODEL,
    )
    .unwrap_or(32_000)
    .max(2_000)
    .min(64_000);
    LlmCallOptions {
        prompt_id: "data_engineer.model_author",
        thread_id: None,
        expected_format: react_core::llm::LlmExpectedFormat::JsonObject,
        temperature: Some(0.12),
        top_p: Some(1.0),
        max_output_tokens: Some(author_max_tokens),
        reasoning_effort: None,
        timeout_secs: None,
    }
};
let pre_mutation_epoch = execution_state.repair.mutation_epoch;
match Agent::run_until_block_non_interactive(
    &registry,
    &actx,
    &sys,
    &tools_card,
    &q,
    llm_options,
)
.await
{
    Ok(RunOutcomeNonInteractive::Complete { .. }) => {
        // Progress is updated at tool-write time; avoid thread-log replay for state.

        // Deterministic invariants: don't advance phases unless the project actually exists.
        let has_proj = control_flow::invariant_has_dbt_project(&actx)
            .await
            .unwrap_or(false);
        let has_models = control_flow::invariant_has_any_models(&actx)
            .await
            .unwrap_or(false);
        if !has_proj || !has_models {
            return Ok(PhaseExecutorOutcome::StayInPhase);
        }
        // Hard cutover: single progress gate controls authoring->validate advancement.
        // Refresh control-state after tool run; tool-side writes in this turn
        // must be visible before we decide whether authoring can advance.
        let gate_state = crate::data_engineer::progress_controller::ExecutionState::load_strict(
            &thread_store,
            thread_id,
        )
        .await?
        .unwrap_or_else(crate::data_engineer::progress_controller::ExecutionState::new);
        if let Err(reason) = crate::data_engineer::progress_controller::gate_authoring_progress(
            &gate_state,
            phase,
        ) {
            apply_guard_block(
                &thread_store,
                thread_id,
                phase,
                GuardBlockKind::AuthoringToValidate,
                reason.clone(),
            )
            .await?;
            return Ok(PhaseExecutorOutcome::StayInPhase);
        }
        if crate::data_engineer::phase_gate::patch_impl_intent_unsatisfied(
            &gate_state,
            phase,
        ) {
            let reason = format!(
                "progress_gate_blocked: review requested implementation patch for phase '{}' and no successful mutation has been recorded since loopback. Apply a mutating file op (patch/rm/mv) before re-validating.",
                phase.as_str()
            );
            apply_guard_block(
                &thread_store,
                thread_id,
                phase,
                GuardBlockKind::AuthoringToValidate,
                reason.clone(),
            )
            .await?;
            return Ok(PhaseExecutorOutcome::StayInPhase);
        }
        if matches!(
            gate_state.repair.pending_patch_impl.as_ref(),
            Some(intent) if intent.phase == phase
        ) {
            crate::data_engineer::state_manager::mutate_execution_state(
                &thread_store,
                thread_id,
                |es| es.clear_pending_patch_impl(),
            )
            .await
            .map_err(|e| {
                format!("failed to clear pending patch-impl intent: {e}")
            })?;
        }

        // Model authoring must actually produce at least one gold model SQL file.
        // Without this, we can "succeed" in silver but never create any gold schema objects.
        if !is_cleanse {
            let actx = Self::agent_tool_ctx(thread_id, sctx);
            if !Self::has_any_gold_model_sql(&actx).await {
                let reason = "No gold models were found under models/core/ or models/marts/ after ModelAuthor. Gold must be explicitly authored (marts/core SQL) before validating/publishing.";
                apply_guard_block(
                    &thread_store,
                    thread_id,
                    phase,
                    GuardBlockKind::MissingGoldModels,
                    reason.to_string(),
                )
                .await?;
                return Ok(PhaseExecutorOutcome::StayInPhase);
            }
        }

        if !track_completion_snapshot_all_done(&actx, track).await {
            return Ok(PhaseExecutorOutcome::StayInPhase);
        }

        let to_phase = if is_cleanse {
            Phase::CleanseValidate
        } else {
            Phase::ModelValidate
        };
        let reason_detail = Self::authoring_complete_reason_detail(
            &thread_store,
            thread_id,
            has_proj,
            has_models,
        )
        .await;
        crate::data_engineer::phase_contract::commit_phase_decision(
            &thread_store,
            thread_id,
            Some(phase),
            crate::data_engineer::phase_contract::PhaseDecision::forward(
                to_phase,
                Some(PhaseReasonCode::AuthoringComplete),
                Some(reason_detail),
            ),
        )
        .await?;
        return Ok(PhaseExecutorOutcome::TransitionCommitted);
    }
    Ok(RunOutcomeNonInteractive::StepBoundary { .. }) => {
        if hard_mutation_repair_mode && phase_guard.last_validate_failed {
            let post_state = crate::data_engineer::progress_controller::ExecutionState::load_strict(
                &thread_store,
                thread_id,
            )
            .await?
            .unwrap_or_else(crate::data_engineer::progress_controller::ExecutionState::new);
            let snapshot = crate::data_engineer::progress_controller::snapshot_authoring_stepboundary_progress(
                hard_mutation_repair_mode,
                phase_guard.last_validate_failed,
                pre_mutation_epoch,
                post_state.repair.mutation_epoch,
                post_state.repair.stall_count,
            );
            match crate::data_engineer::authoring_driver::AuthoringDriver::run_turn(
                &authoring_ctx,
                &snapshot,
            ) {
                crate::data_engineer::authoring_driver::AuthoringTurnResult::HardError {
                    message,
                } => {
                    let violation = crate::data_engineer::progress_controller::PlanViolation::new(
                        phase,
                        None,
                        format!("Authoring repair stall: {message}"),
                    );
                    crate::data_engineer::phase_contract::commit_plan_revision_loopback(
                        &thread_store, thread_id, phase, vec![violation],
                        crate::data_engineer::progress_controller::PlanRevisionStrategy::Rewrite,
                    ).await?;
                    return Ok(PhaseExecutorOutcome::TransitionCommitted);
                }
                crate::data_engineer::authoring_driver::AuthoringTurnResult::Continue => {}
            }
        }
        return Ok(PhaseExecutorOutcome::StayInPhase);
    }
    Err(e) => return Err(e),
}
                
    }
}
