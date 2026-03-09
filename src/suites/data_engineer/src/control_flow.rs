use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::time::timeout;
use tracing::warn;

use react_core::agent::AgentCtx;
use crate::providers::DbtValidateArgs;
use react_core::session::{ThreadStore, ToolObservation, ToolStepMeta};
use react_core::tools::Tool;

use crate::tools::files_tool::FilesTool;
use crate::dbt;
pub use react_core::workflow::TransitionIntent;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Preflight,
    CleansePlan,
    CleanseAuthor,
    CleanseValidate,
    CleanseReview,
    ModelPlan,
    ModelAuthor,
    ModelValidate,
    ModelReview,
    PublishAwaitApproval,
    Publish,
    PostPublishReview,
    Done,
}

#[cfg(test)]
mod state_first_tests {
    use super::*;

    #[test]
    fn phase_as_str_from_str_round_trip() {
        for phase in ALL_PHASES {
            let s = phase.as_str();
            let back = Phase::from_str(s).unwrap_or_else(|| panic!("from_str failed for {s}"));
            assert_eq!(back, phase, "round-trip failed for {s}");
        }
    }

    #[test]
    fn phase_ordinal_is_sequential() {
        for (i, phase) in ALL_PHASES.iter().enumerate() {
            assert_eq!(phase.ordinal(), i, "ordinal mismatch for {:?}", phase);
        }
    }

    #[test]
    fn phase_serde_round_trip() {
        for phase in ALL_PHASES {
            let json = serde_json::to_string(&phase).unwrap();
            let back: Phase = serde_json::from_str(&json).unwrap();
            assert_eq!(back, phase, "serde round-trip failed for {:?}", phase);
            assert_eq!(
                json.trim_matches('"'),
                phase.as_str(),
                "serde name does not match as_str for {:?}",
                phase,
            );
        }
    }

    #[test]
    fn derive_guard_state_from_execution_state_marks_validate_failure() {
        let mut st = crate::progress_controller::ExecutionState::new();
        st.telemetry.last_validate = Some(crate::progress_controller::LastValidateState {
            ok: Some(false),
            ..crate::progress_controller::LastValidateState::default()
        });
        let guard = derive_guard_state_from_execution_state(&st);
        assert!(guard.last_validate_failed);
        assert!(!guard.mutated_since_fail);
    }

    #[test]
    fn derive_guard_state_from_execution_state_tracks_mutation_progress() {
        let mut st = crate::progress_controller::ExecutionState::new();
        st.telemetry.last_validate = Some(crate::progress_controller::LastValidateState {
            ok: Some(false),
            ..crate::progress_controller::LastValidateState::default()
        });
        st.repair.last_progress_delta = Some(crate::progress_controller::ProgressDelta {
            target_hash_changed: true,
            failed_target_count_delta: 0,
            failure_signature_changed: false,
            checklist_completed_delta: 0,
            progress_made: true,
        });
        let guard = derive_guard_state_from_execution_state(&st);
        assert!(guard.last_validate_failed);
        assert!(guard.mutated_since_fail);
    }
}

pub const ALL_PHASES: [Phase; 13] = [
    Phase::Preflight,
    Phase::CleansePlan,
    Phase::CleanseAuthor,
    Phase::CleanseValidate,
    Phase::CleanseReview,
    Phase::ModelPlan,
    Phase::ModelAuthor,
    Phase::ModelValidate,
    Phase::ModelReview,
    Phase::PublishAwaitApproval,
    Phase::Publish,
    Phase::PostPublishReview,
    Phase::Done,
];

impl Phase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::Preflight => "preflight",
            Phase::CleansePlan => "cleanse_plan",
            Phase::CleanseAuthor => "cleanse_author",
            Phase::CleanseValidate => "cleanse_validate",
            Phase::CleanseReview => "cleanse_review",
            Phase::ModelPlan => "model_plan",
            Phase::ModelAuthor => "model_author",
            Phase::ModelValidate => "model_validate",
            Phase::ModelReview => "model_review",
            Phase::PublishAwaitApproval => "publish_await_approval",
            Phase::Publish => "publish",
            Phase::PostPublishReview => "post_publish_review",
            Phase::Done => "done",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn ordinal(&self) -> usize {
        ALL_PHASES.iter().position(|p| p == self).unwrap_or(0)
    }
}

impl std::str::FromStr for Phase {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "preflight" => Ok(Phase::Preflight),
            "cleanse_plan" => Ok(Phase::CleansePlan),
            "cleanse_author" => Ok(Phase::CleanseAuthor),
            "cleanse_validate" => Ok(Phase::CleanseValidate),
            "cleanse_review" => Ok(Phase::CleanseReview),
            "model_plan" => Ok(Phase::ModelPlan),
            "model_author" => Ok(Phase::ModelAuthor),
            "model_validate" => Ok(Phase::ModelValidate),
            "model_review" => Ok(Phase::ModelReview),
            "publish_await_approval" => Ok(Phase::PublishAwaitApproval),
            "publish" => Ok(Phase::Publish),
            "post_publish_review" => Ok(Phase::PostPublishReview),
            "done" => Ok(Phase::Done),
            _ => Err(format!("unknown phase: '{}'", s)),
        }
    }
}

pub(crate) fn allowed_next_phases(from: Phase) -> &'static [Phase] {
    match from {
        Phase::Preflight => &[Phase::CleansePlan],
        Phase::CleansePlan => &[Phase::CleansePlan, Phase::CleanseAuthor],
        Phase::CleanseAuthor => &[Phase::CleanseAuthor, Phase::CleanseValidate, Phase::CleansePlan],
        Phase::CleanseValidate => &[
            Phase::CleanseValidate,
            Phase::CleanseAuthor,
            Phase::CleanseReview,
            Phase::CleansePlan,
        ],
        Phase::CleanseReview => &[
            Phase::CleanseReview,
            Phase::CleansePlan,
            Phase::CleanseAuthor,
            Phase::ModelPlan,
        ],
        Phase::ModelPlan => &[Phase::ModelPlan, Phase::ModelAuthor],
        Phase::ModelAuthor => &[Phase::ModelAuthor, Phase::ModelValidate, Phase::ModelPlan],
        Phase::ModelValidate => &[
            Phase::ModelValidate,
            Phase::ModelAuthor,
            Phase::ModelReview,
            Phase::ModelPlan,
        ],
        Phase::ModelReview => &[
            Phase::ModelReview,
            Phase::ModelPlan,
            Phase::ModelAuthor,
            Phase::PublishAwaitApproval,
        ],
        Phase::PublishAwaitApproval => {
            &[Phase::PublishAwaitApproval, Phase::Publish, Phase::ModelReview]
        }
        Phase::Publish => &[Phase::Publish, Phase::PostPublishReview, Phase::ModelReview],
        Phase::PostPublishReview => &[
            Phase::PostPublishReview,
            Phase::ModelPlan,
            Phase::ModelAuthor,
            Phase::PublishAwaitApproval,
            Phase::Done,
        ],
        Phase::Done => &[Phase::Done],
    }
}


pub(crate) fn replan_backtrack_counter_cap() -> usize {
    crate::env_util::max_replan_backtracks()
}

#[derive(Clone, Debug, Default)]
pub struct DerivedGuardState {
    pub last_validate_failed: bool,
    pub mutated_since_fail: bool,
    /// True if at least one successful file op=patch occurred since the failing validate,
    /// even if it ended up being a no-op write (mutated=false).
    ///
    /// This is used as a conservative "we did try to apply a fix" signal to avoid deadlocking
    /// the suite purely due to mutation detection brittleness.
    pub patched_since_fail: bool,
    pub mutation_failures_since_validate: usize,
    pub probe_required: bool,
    pub probe_satisfied: bool,
}

pub fn derive_guard_state_from_execution_state(
    st: &crate::progress_controller::ExecutionState,
) -> DerivedGuardState {
    let last_validate_failed = st
        .telemetry
        .last_validate
        .as_ref()
        .and_then(|lv| lv.ok)
        == Some(false);
    let mutated_since_fail = st
        .repair
        .last_progress_delta
        .as_ref()
        .map(|d| d.target_hash_changed || d.progress_made)
        .unwrap_or(false);
    let patched_since_fail = st
        .telemetry
        .last_mutation_summary
        .as_ref()
        .map(|m| matches!(m.op, crate::progress_controller::MutationOp::Patch))
        .unwrap_or(false);
    let probe_status = st.probe_requirement_status();
    let (probe_required, probe_satisfied) = match probe_status {
        crate::progress_controller::ProbeRequirementStatus::NotRequired => {
            (false, true)
        }
        crate::progress_controller::ProbeRequirementStatus::Required => {
            (true, false)
        }
        crate::progress_controller::ProbeRequirementStatus::Allowed => {
            (true, true)
        }
        crate::progress_controller::ProbeRequirementStatus::ExhaustedRequireMutation => {
            (false, true)
        }
    };
    DerivedGuardState {
        last_validate_failed,
        mutated_since_fail,
        patched_since_fail,
        mutation_failures_since_validate: st.consecutive_noop_patches(),
        probe_required,
        probe_satisfied,
    }
}

pub(crate) fn is_replan_backtrack(from: Phase, to: Phase) -> bool {
    use crate::track_spec::TrackKind;
    // PostPublishReview can backtrack into the model track
    let from_track = TrackKind::from_any_phase(from)
        .or(if from == Phase::PostPublishReview { Some(TrackKind::Model) } else { None });
    let Some(from_track) = from_track else {
        return false;
    };
    let Some(to_track) = TrackKind::from_any_phase(to) else {
        return false;
    };
    if from_track != to_track {
        return false;
    }
    let plan = from_track.plan_phase();
    let author = from_track.author_phase();
    from != plan && (to == plan || to == author)
}

#[derive(Clone, Debug)]
pub enum AuthoringGate {
    Allow,
    Block { reason: String },
}

pub fn gate_author_phase_execution(plan: &impl crate::plan_types::TrackPlan) -> AuthoringGate {
    let issues = plan.executable_plan_issues();
    if issues.is_empty() {
        return AuthoringGate::Allow;
    }
    let mut msg = String::from(
        "Approved plan is not executable. Re-enter planning before authoring:\n",
    );
    for issue in issues.iter().take(8) {
        msg.push_str("- ");
        msg.push_str(issue);
        msg.push('\n');
    }
    AuthoringGate::Block {
        reason: msg.trim().to_string(),
    }
}

/// Unified deterministic dbt validation. When `select` is `Some`, runs targeted
/// validation for a specific set of models; otherwise validates the whole project.
pub(crate) struct DeterministicDbtValidateOnce;

impl DeterministicDbtValidateOnce {
    pub async fn run(
        ctx: &AgentCtx,
        build: bool,
        run: bool,
        dataset_ids: Option<&[String]>,
    ) -> Result<crate::controller_event::ValidateObservationContract, String> {
        Self::run_inner(ctx, build, run, None, dataset_ids).await
    }

    async fn run_inner(
        ctx: &AgentCtx,
        build: bool,
        run: bool,
        select: Option<&[String]>,
        dataset_ids: Option<&[String]>,
    ) -> Result<crate::controller_event::ValidateObservationContract, String> {
        let dbt = crate::ctx_ext::actx_dbt(ctx)
            .ok_or_else(|| "dbt provider missing".to_string())?;
        let Some(cfg) = crate::resolved_config_from_ctx(ctx) else {
            return Err(
                "resolved_config missing (needed to generate profiles.yml deterministically)"
                    .to_string(),
            );
        };
        let threads = crate::ctx_ext::actx_query(ctx).as_ref().map(|q| q.max_concurrency());
        let gen = dbt::profile::generate_profiles_yml(cfg, threads)?;
        let td = tempfile::tempdir().map_err(|e| e.to_string())?;
        let profiles_dir = td.path().to_string_lossy().to_string();
        let profiles_path = td.path().join("profiles.yml");
        std::fs::write(&profiles_path, gen.profiles_yml.as_bytes()).map_err(|e| e.to_string())?;

        let res = dbt
            .validate_project(
                ctx.scope(),
                &DbtValidateArgs {
                    project_name: crate::env_util::SUITE_PROJECT_NAME.to_string(),
                    profiles_dir: Some(profiles_dir),
                    target: gen.target,
                    run,
                    build,
                    select: select.map(|s| s.to_vec()),
                    exclude: None,
                },
            )
            .await?;

        let mut v = serde_json::to_value(res).unwrap_or_else(
            |_| serde_json::json!({"ok": false, "error": "failed to serialize result"}),
        );
        if let Some(obj) = v.as_object_mut() {
            obj.insert(
                "dialect".to_string(),
                serde_json::json!(
                    crate::dbt_repair::remediate::active_provider_dialect(cfg)
                ),
            );
            let rf = crate::dbt_error::extract_runtime_failures_from_logs(
                &obj.get("logs").cloned().unwrap_or(Value::Null),
            );
            obj.insert("runtime_failures".to_string(), serde_json::json!(rf));
            if let Some(sel) = select {
                obj.insert("select".to_string(), serde_json::json!(sel));
            }
            let ok = obj.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
            if !ok {
                let errors: Vec<String> = obj
                    .get("errors")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let logs = obj.get("logs").cloned().unwrap_or(Value::Null);
                if let Ok(sum) = crate::dbt_error::summarize_dbt_failure_llm(
                    ctx,
                    &errors,
                    &logs,
                    &rf,
                    2000,
                ).await {
                    obj.insert("error_summary".to_string(), serde_json::json!(sum.summary));
                    obj.insert(
                        "failing_nodes".to_string(),
                        serde_json::json!(sum.failing_nodes),
                    );
                    obj.insert(
                        "suggested_next_files".to_string(),
                        serde_json::json!(sum.suggested_next_files),
                    );
                }
            }
            if let Some(ds) = dataset_ids {
                obj.insert("dataset_ids".to_string(), serde_json::json!(ds));
            }
        }
        crate::controller_event::validate_contract_from_observation(v)
    }
}

fn extract_string_vec_from_extra(extra: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    extra
        .get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn de_clean_tool_name(name: &str, args: &Value) -> String {
    match name {
        "file" => {
            let op = args.get("op").and_then(|v| v.as_str()).unwrap_or("");
            match op {
                "get" => {
                    let p = args.get("path").and_then(|v| v.as_str()).unwrap_or("").trim();
                    if !p.is_empty() { return format!("Read {p}"); }
                    "Read file".to_string()
                }
                "list" => {
                    let p = args.get("prefix").and_then(|v| v.as_str()).unwrap_or("").trim();
                    if !p.is_empty() { return format!("List {p}"); }
                    "List files".to_string()
                }
                "patch" => {
                    let p = args.get("path").and_then(|v| v.as_str()).unwrap_or("").trim();
                    if !p.is_empty() { return format!("Patch {p}"); }
                    "Patch file".to_string()
                }
                _ => {
                    if !op.is_empty() { return format!("file {op}"); }
                    "file".to_string()
                }
            }
        }
        "json_file" => {
            let p = args.get("path").and_then(|v| v.as_str()).unwrap_or("").trim();
            if !p.is_empty() { format!("JSON {p}") } else { "JSON file".to_string() }
        }
        "sql_schema" => {
            let t = args.get("table").and_then(|v| v.as_str()).unwrap_or("").trim();
            if !t.is_empty() { format!("Describe {t}") } else { "List tables".to_string() }
        }
        "sql_stats" => {
            let t = args.get("table").and_then(|v| v.as_str()).unwrap_or("").trim();
            if !t.is_empty() { format!("Stats {t}") } else { "Stats".to_string() }
        }
        "sql_sample" => {
            let t = args.get("table").and_then(|v| v.as_str()).unwrap_or("").trim();
            if !t.is_empty() { format!("Sample {t}") } else { "Sample".to_string() }
        }
        "run_sql" => "Run SQL".to_string(),
        other => other.replace('_', " "),
    }
}

pub async fn call_and_record_tool(
    store: &ThreadStore,
    thread_id: &str,
    agent: Option<String>,
    tool: &dyn Tool,
    args: Value,
    ctx: &AgentCtx,
    timeout_secs: u64,
) -> Value {
    let agent_str = agent.unwrap_or_else(|| crate::env_util::UNKNOWN_AGENT.to_string());
    let tool_name = tool.name().to_string();
    let meta = ToolStepMeta {
        agent: agent_str,
        phase: "suite_tool_dispatch".to_string(),
        name: tool_name.clone(),
        clean_name: de_clean_tool_name(tool.name(), &args),
        args: args.clone(),
        ctx: ctx.exec_ctx().clone(),
    };

    store
        .run_observed(
            thread_id,
            meta,
            || async {
                let raw = match timeout(
                    Duration::from_secs(timeout_secs.max(1)),
                    tool.call(args.clone(), ctx),
                )
                .await
                {
                    Ok(r) => r.unwrap_or_else(|e| serde_json::json!({"ok": false, "errors": [e]})),
                    Err(_) => serde_json::json!({"ok": false, "errors": ["tool timeout"]}),
                };

                let obs = ToolObservation::normalize(raw.clone());
                record_post_tool_telemetry(store, thread_id, &tool_name, &args, &obs).await;
                Ok(raw)
            },
            |raw: &Value| Ok(raw.clone()),
        )
        .await
        .unwrap_or_else(|e| serde_json::json!({"ok": false, "errors": [e]}))
}

async fn record_post_tool_telemetry(
    store: &ThreadStore,
    thread_id: &str,
    tool_name: &str,
    args: &Value,
    obs: &ToolObservation,
) {
    if tool_name == "json_file" {
        let manifest_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or("");
        if let Some(path_kind) =
            crate::progress_controller::classify_manifest_lookup_path(manifest_path)
        {
            match crate::state_manager::load_execution_state_strict(
                &store.control_store(),
                thread_id,
            )
            .await
            {
                Ok(Some(mut st)) => {
                    if st.phase.current_phase == Some(Phase::ModelPlan) {
                        let failure_kind = if obs.ok {
                            None
                        } else {
                            crate::progress_controller::classify_manifest_lookup_failure(
                                &obs.errors,
                            )
                        };
                        st.note_manifest_lookup_attempt(path_kind, obs.ok, failure_kind);
                        if let Err(e) = crate::state_manager::replace_execution_state(
                            &store.control_store(),
                            thread_id,
                            st,
                        )
                        .await
                        {
                            warn!("failed to persist manifest lookup telemetry: {}", e);
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    warn!(
                        "failed to load strict execution state for manifest telemetry: {}",
                        e
                    );
                }
            }
        }
    }

    if let Some((op, affected_paths)) = (|| {
        let key = match tool_name {
            "apply_next_cleanse_batch" | "apply_next_cleanse_schema_batch" => {
                "succeeded_dataset_ids"
            }
            "apply_next_model_batch" | "apply_next_model_schema_batch" => "succeeded_item_names",
            "staging_model" | "gold_model" => "written_keys",
            _ => return None,
        };
        let items = extract_string_vec_from_extra(&obs.extra, key);
        if items.is_empty() {
            None
        } else {
            Some((crate::progress_controller::MutationOp::Patch, items))
        }
    })() {
        match crate::state_manager::load_execution_state_strict(
            &store.control_store(),
            thread_id,
        )
        .await
        {
            Ok(Some(mut st)) => {
                st.set_last_mutation_summary(op, affected_paths, Vec::new());
                if let Err(e) = crate::state_manager::replace_execution_state(
                    &store.control_store(),
                    thread_id,
                    st,
                )
                .await
                {
                    warn!("failed to persist non-file mutation summary: {}", e);
                }
            }
            Ok(None) => {}
            Err(e) => {
                warn!(
                    "failed to load strict execution state for mutation summary: {}",
                    e
                );
            }
        }
    }
}

/// Deterministic authoring invariant: ensure there is at least one model SQL file in `models/`.
pub async fn invariant_has_any_models(ctx: &AgentCtx) -> Result<bool, String> {
    let tool = FilesTool { datasets: None };
    let obs = tool
        .call(
            serde_json::json!({"op":"list","prefix":"models/","limit":crate::env_util::FILE_LIST_LIMIT}),
            ctx,
        )
        .await
        .unwrap_or_else(|e| serde_json::json!({"ok": false, "error": e}));
    if obs.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        warn!("file list failed: {:?}", obs.get("error"));
        return Ok(false);
    }
    let items = obs
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut n_sql = 0usize;
    for it in items {
        let Some(p) = it.get("path").and_then(|v| v.as_str()) else {
            continue;
        };
        if p.starts_with("models/") && p.ends_with(".sql") && !p.contains("/_versions/") {
            n_sql += 1;
        }
    }
    Ok(n_sql > 0)
}

/// Deterministic invariant: dbt_project.yml exists in the scoped dbt project.
pub async fn invariant_has_dbt_project(ctx: &AgentCtx) -> Result<bool, String> {
    let tool = FilesTool { datasets: None };
    let obs = tool
        .call(
            serde_json::json!({"op":"get","path":"dbt_project.yml","max_chars":crate::env_util::FILE_GET_MAX_CHARS}),
            ctx,
        )
        .await
        .unwrap_or_else(|e| serde_json::json!({"ok": false, "error": e}));
    Ok(obs.get("ok").and_then(|v| v.as_bool()) == Some(true))
}

