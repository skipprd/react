use async_trait::async_trait;

use self::policy_sql_validated::SqlValidatedPolicy;
use self::policy_sql_validated::DatasetCandidate;
use self::preflight::PreflightProvider;
use react_core::suite::{FlowFrame, FlowKind, Suite, SuiteCtx};
use react_core::agent::{
    Agent, AgentCtx, AgentPolicy, InterruptKind, RunOutcome, RunOutcomeNonInteractive,
};
use crate::data_engineer::domain_types::{GuardBlockKind, PhaseReasonCode};
use react_core::keyspace::encode_key_component;
use react_core::llm::LlmCallOptions;
use react_core::session::{ThreadBootstrapState, ThreadStore, ToolStepStatus};
use react_core::tools::ToolRegistry;
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;
use crate::data_engineer::phase_contract::{
    commit_guard_block as apply_guard_block, plan_status_reason_detail,
};

pub struct DataEngineerSuite;

pub(crate) fn resolved_config_from_ctx(ctx: &AgentCtx) -> Option<&react_core::resolved_config::ReactResolvedConfig> {
    ctx.resolved_config.as_ref().map(|c| c.as_ref())
}

impl react_core::suite::WorkflowSuiteContract for DataEngineerSuite {
    type Phase = control_flow::Phase;
    type ReasonCode = PhaseReasonCode;
    type GuardKind = GuardBlockKind;
    type State = crate::data_engineer::progress_controller::ExecutionState;
    type Event = crate::data_engineer::progress_controller::DataEngineerEvent;

    fn phase_as_str(phase: Self::Phase) -> &'static str {
        phase.as_str()
    }

    fn reason_as_str(reason: Self::ReasonCode) -> &'static str {
        reason.as_str()
    }

    fn guard_kind_as_str(kind: Self::GuardKind) -> &'static str {
        kind.as_str()
    }

    fn is_backtrack(from: Self::Phase, to: Self::Phase) -> bool {
        control_flow::is_replan_backtrack(from, to)
    }

    fn replan_backtrack_cap() -> usize {
        control_flow::replan_backtrack_counter_cap()
    }

    fn pre_turn(
        state: &crate::data_engineer::progress_controller::ExecutionState,
    ) -> react_core::workflow::PreTurnDirective<Self::GuardKind> {
        let phase = state
            .phase_state()
            .current_phase
            .unwrap_or(control_flow::Phase::Preflight);
        crate::data_engineer::phase_gate::evaluate_pre_turn_directive(
            state,
            phase,
            control_flow::replan_backtrack_counter_cap(),
        )
    }

    fn reduce(
        state: &mut crate::data_engineer::progress_controller::ExecutionState,
        event: crate::data_engineer::progress_controller::DataEngineerEvent,
    ) {
        state.apply_event(event);
    }
}

impl react_core::suite::WorkflowNodeContract for DataEngineerSuite {
    type Node = crate::data_engineer::control_flow::Phase;

    fn node_from_state(
        state: &crate::data_engineer::progress_controller::ExecutionState,
    ) -> Self::Node {
        state.phase.current_phase.unwrap_or(crate::data_engineer::control_flow::Phase::Preflight)
    }

    fn phase_from_node(node: Self::Node) -> Self::Phase {
        node
    }
}

pub mod providers;
pub mod env_util;
pub mod de_config;
pub mod ctx_ext;
pub mod controller_event;
pub mod controller_kernel;
pub mod control_flow;
pub mod domain_types;
pub mod thread_cache;
pub mod dataset_truth;
pub mod dbt;
pub mod dbt_error;
pub mod dbt_repair;
pub mod facts;
pub mod naming;
pub mod mutation_gateway;
pub mod patch_contract;
pub mod patch_protocol;
pub mod phase_contract;
pub mod phase_gate;
pub mod phase_reason_detail;
mod phase_author;
mod phase_author_lifecycle;
mod phase_plan;
mod phase_plan_lifecycle;
mod phase_preflight;
mod phase_publish;
mod phase_review;
mod phase_validate;
mod plan_review_helpers;
mod plan_grounding;
pub(crate) mod plan_progress;
mod plan_storage;
mod plan_types;
mod plan_validation;
pub mod plan;
pub mod ws_plans;
pub mod plan_kind;
pub mod plan_schema;
pub mod probe_target;
pub mod progress_controller;
pub mod project_fs;
pub mod prompt_packets;
pub mod prompts;
pub mod authoring_ir;
pub mod authoring_driver;
pub mod chunk_progress_contract;
pub mod references;
mod review_batched;
mod review_prompts;
mod review_persistence;
pub mod retry_budget;
pub mod schema_policy;
pub mod sql_first;
pub mod state_manager;
pub mod tool_ops;
mod tool_policies;
mod tool_registry_builder;
mod track_spec;
pub mod transition_dispatcher;
pub mod tools;
pub mod policy_sql_validated;
pub mod preflight;
mod llm_profiles;
mod enrichment;
mod catalog_bootstrap;
mod agent_modes;
pub(crate) use track_spec::TrackKind;
pub use ctx_ext::copy_capabilities_to_actx;
use llm_profiles::PlanningLlmProfile;
use agent_modes::{AgentMode, AgentToolCapability};

pub(crate) enum PhaseExecutorOutcome {
    /// Phase still active, run another iteration.
    StayInPhase,
    /// Transition was committed; reload state and continue.
    TransitionCommitted,
    /// Phase complete; return these frames to the caller.
    Return(Vec<FlowFrame>),
}

/// Interrupt policy used by non-deterministic single-pass modes.
///
/// Deterministic agent-mode orchestration enforces its own non-interactive contract.
struct InterruptOnlyPolicy;

#[async_trait::async_trait]
impl AgentPolicy for InterruptOnlyPolicy {
    fn interrupt_for_action(
        &self,
        action_name: &str,
        args: &serde_json::Value,
        obs: &serde_json::Value,
    ) -> Option<(InterruptKind, String)> {
        if action_name == "ask_user" {
            let prompt = args
                .get("prompt")
                .and_then(|x| x.as_str())
                .or_else(|| obs.get("prompt").and_then(|x| x.as_str()))
                .unwrap_or("Please provide additional context.")
                .to_string();
            return Some((InterruptKind::AwaitUser, prompt));
        }
        if action_name == "ask_approval" {
            let prompt = args
                .get("prompt")
                .and_then(|x| x.as_str())
                .or_else(|| obs.get("prompt").and_then(|x| x.as_str()))
                .unwrap_or("Please review and approve/reject.")
                .to_string();
            return Some((InterruptKind::AwaitApproval, prompt));
        }
        None
    }

    fn clean_tool_name(&self, name: &str, args: &serde_json::Value) -> String {
        crate::data_engineer::control_flow::de_clean_tool_name(name, args)
    }

    fn timeout_for_tool(&self, action_name: &str) -> Option<u64> {
        use crate::data_engineer::env_util::{
            TOOL_TIMEOUT_FAST_SECS, TOOL_TIMEOUT_MEDIUM_SECS,
            TOOL_TIMEOUT_SLOW_SECS, TOOL_TIMEOUT_EXTRA_SLOW_SECS,
        };
        let secs = match action_name {
            "staging_model" | "apply_next_cleanse_batch" | "apply_next_cleanse_schema_batch" =>
                TOOL_TIMEOUT_SLOW_SECS,

            "gold_model" | "apply_next_model_batch" | "apply_next_model_schema_batch" =>
                TOOL_TIMEOUT_MEDIUM_SECS,

            "dbt_validate" | "publish_dbt_to_provider" =>
                TOOL_TIMEOUT_EXTRA_SLOW_SECS,

            "preflight_catalog_all" => TOOL_TIMEOUT_SLOW_SECS,
            "preflight_catalog_dataset" | "preflight_catalog_schema" => TOOL_TIMEOUT_MEDIUM_SECS,

            _ => TOOL_TIMEOUT_FAST_SECS,
        };
        Some(secs)
    }

    async fn handle_complete(
        &self,
        tools: &ToolRegistry,
        ctx: &AgentCtx,
        transcript: &mut Vec<String>,
        store: Option<&react_core::session::ThreadStore>,
        thread_id: &str,
        complete_env: &react_core::agent::CompleteEnvelope,
    ) -> Result<Option<RunOutcome>, String> {
        react_core::agent::DefaultPolicy
            .handle_complete(tools, ctx, transcript, store, thread_id, complete_env)
            .await
    }
}

#[cfg(test)]
mod interrupt_only_policy_tests {
    use super::*;

    #[test]
    fn interrupt_only_policy_overrides_gold_model_timeout() {
        let p = InterruptOnlyPolicy;
        assert_eq!(p.timeout_for_tool("gold_model"), Some(300));
    }
}


#[derive(Clone, Debug)]
enum PlanState {
    CleanseSqlDatasetIds(Vec<String>),
    CleanseSchemaDatasetIds(Vec<String>),
    ModelSqlItemNames(Vec<String>),
    ModelSchemaItemNames(Vec<String>),
    Unconstrained,
}

#[derive(Clone, Debug)]
struct NonEmptyCleanseDatasetIds {
    ids: Vec<String>,
}

impl NonEmptyCleanseDatasetIds {
    fn from_discovered_raw(discovered_raw: &BTreeSet<String>) -> Result<Self, String> {
        let ids: Vec<String> = discovered_raw
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if ids.is_empty() {
            return Err(
                "cleanse planning has no discovered raw datasets; cannot build deterministic task skeleton"
                    .to_string(),
            );
        }
        Ok(Self { ids })
    }

    fn as_slice(&self) -> &[String] {
        &self.ids
    }
}

impl DataEngineerSuite {

    fn first_column_name_from_sql_schema_observation(obs: &serde_json::Value) -> Option<String> {
        obs.get("columns")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter().find_map(|c| {
                    c.get("name")
                        .and_then(|x| x.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                })
            })
    }

    fn enforce_cleanse_plan_raw_only(plan: &mut crate::data_engineer::plan::CleansePlan) -> usize {
        let before = plan.tasks.len();
        plan.tasks
            .retain(|t| Self::is_raw_dataset_id(t.dataset_id.trim()));
        let keep: HashSet<String> = plan
            .tasks
            .iter()
            .map(|t| t.dataset_id.trim().to_string())
            .collect();
        for b in plan.batches.iter_mut() {
            b.retain(|ds| keep.contains(ds.trim()));
        }
        plan.batches.retain(|b| !b.is_empty());
        before.saturating_sub(plan.tasks.len())
    }

    fn synthesize_cleanse_plan_from_grounded_raw(
        plan: &mut crate::data_engineer::plan::CleansePlan,
        allowed_raw: &BTreeSet<String>,
    ) -> bool {
        if allowed_raw.is_empty() {
            return false;
        }
        let ids: Vec<String> = allowed_raw.iter().cloned().collect();
        let tasks = ids
            .iter()
            .map(|dataset_id| crate::data_engineer::plan::CleanseTask {
                dataset_id: dataset_id.clone(),
                expected_model_path: None,
                invariants: vec![],
                implementation_spec: Some(crate::data_engineer::plan::CleanseImplementationSpec {
                    spec_version: 1,
                    row_preserving: true,
                    output_fields: vec![],
                    prohibited_ops: vec![],
                }),
                status: crate::data_engineer::plan::TaskStatus::Pending,
                checklist: crate::data_engineer::plan::canonical_task_checklist(TrackKind::Cleanse),
            })
            .collect::<Vec<_>>();
        let batches = ids.chunks(plan_progress::MAX_BATCH_SIZE).map(|c| c.to_vec()).collect::<Vec<_>>();
        plan.tasks = tasks;
        plan.batches = batches;
        plan.work_groups = crate::data_engineer::plan::canonical_work_groups_from_batches(
            &plan.batches,
            "cleanse",
        );
        true
    }

    fn collect_cleanse_grounding_candidates(
        plan: &crate::data_engineer::plan::CleansePlan,
        discovered_raw: &BTreeSet<String>,
    ) -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for t in plan.tasks.iter() {
            let id = t.dataset_id.trim();
            if !id.is_empty() {
                out.insert(id.to_string());
            }
        }
        for b in plan.batches.iter() {
            for ds in b.iter() {
                let id = ds.trim();
                if !id.is_empty() {
                    out.insert(id.to_string());
                }
            }
        }
        // Deterministic safety rail: when the skeleton parser yields no task ids,
        // seed grounding from observed raw relations in this plan phase.
        if out.is_empty() {
            out.extend(discovered_raw.iter().cloned());
        }
        out.into_iter().collect()
    }

    fn deterministic_cleanse_skeleton_from_discovered_raw(
        discovered_raw: &BTreeSet<String>,
    ) -> Result<crate::data_engineer::plan_schema::CleansePlanSkeletonV1, String> {
        let ids = NonEmptyCleanseDatasetIds::from_discovered_raw(discovered_raw)?;
        let tasks = ids
            .as_slice()
            .iter()
            .map(|dataset_id| crate::data_engineer::plan_schema::CleansePlanSkeletonTaskV1 {
                dataset_id: dataset_id.clone(),
            })
            .collect::<Vec<_>>();
        let batches = ids
            .as_slice()
            .chunks(plan_progress::MAX_BATCH_SIZE)
            .map(|chunk| chunk.to_vec())
            .collect::<Vec<_>>();
        Ok(crate::data_engineer::plan_schema::CleansePlanSkeletonV1 { tasks, batches })
    }

    fn is_raw_dataset_id(dataset_id: &str) -> bool {
        let Some(ds) = crate::data_engineer::references::DatasetRef::parse(dataset_id) else {
            return false;
        };
        let schema = ds.schema.to_ascii_lowercase();
        let table = ds.table.to_ascii_lowercase();
        schema.contains("raw") || table.starts_with("raw_")
    }


    async fn discovered_raw_relations_from_catalog(
        datasets: Option<&Arc<dyn crate::data_engineer::providers::DatasetCatalogProvider>>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let Some(ds) = datasets else {
            return out;
        };
        let Ok(items) = ds.list_datasets().await else {
            return out;
        };
        for item in items {
            let fqn = item.fqn();
            if Self::is_raw_dataset_id(&fqn) {
                out.insert(fqn);
            }
        }
        out
    }

    async fn run_deterministic_probe_for_table(
        thread_store: &ThreadStore,
        thread_id: &str,
        actx: &AgentCtx,
        sql_schema_tool: &tools::sql_schema::SqlSchemaTool,
        sql_stats_tool: &tools::sql_stats::SqlStatsTool,
        sql_sample_tool: &tools::sql_sample::SqlSampleTool,
        run_sql_tool: &tools::sql_run::SqlRunTool,
        table: &str,
        sql_schema_timeout: u64,
        sql_stats_timeout: u64,
        sql_sample_timeout: u64,
        run_sql_timeout: u64,
    ) -> (bool, Option<String>) {
        let schema_obs = control_flow::call_and_record_tool(
            thread_store,
            thread_id,
            Some(env_util::DEFAULT_AGENT_NAME.to_string()),
            sql_schema_tool,
            serde_json::json!({"table": table}),
            actx,
            sql_schema_timeout,
        )
        .await;
        let first_field = Self::first_column_name_from_sql_schema_observation(&schema_obs);
        if let Some(field) = first_field.as_ref() {
            let stats_obs = control_flow::call_and_record_tool(
                thread_store,
                thread_id,
                Some(env_util::DEFAULT_AGENT_NAME.to_string()),
                sql_stats_tool,
                serde_json::json!({"table": table, "field": field}),
                actx,
                sql_stats_timeout,
            )
            .await;
            if stats_obs
                .get("ok")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return (true, Some(field.clone()));
            }
            let sample_obs = control_flow::call_and_record_tool(
                thread_store,
                thread_id,
                Some(env_util::DEFAULT_AGENT_NAME.to_string()),
                sql_sample_tool,
                serde_json::json!({"table": table, "field": field, "k": 10}),
                actx,
                sql_sample_timeout,
            )
            .await;
            if sample_obs
                .get("ok")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return (true, Some(field.clone()));
            }
        }
        let run_obs = control_flow::call_and_record_tool(
            thread_store,
            thread_id,
            Some(env_util::DEFAULT_AGENT_NAME.to_string()),
            run_sql_tool,
            serde_json::json!({"sql": format!("SELECT count(*) AS total FROM {}", table)}),
            actx,
            run_sql_timeout,
        )
        .await;
        (
            run_obs.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
            first_field,
        )
    }

    fn churn_audit_acceptance_criteria() -> serde_json::Value {
        serde_json::json!({
            "no_false_completion": "plans are never marked completed while checklist items remain pending",
            "no_non_executable_authoring": "authoring exits to planning when executable work-group structure is invalid",
            "no_review_churn_from_missing_staging": "model plan grounding loops must not be caused by skipped upstream authoring",
        })
    }

    fn excerpt(s: &str, max_chars: usize) -> String {
        if s.chars().count() <= max_chars {
            return s.to_string();
        }
        let mut out = s.chars().take(max_chars).collect::<String>();
        out.push_str("\n... (truncated)");
        out
    }

    fn parse_json_object_strict(raw: &str) -> Result<serde_json::Value, String> {
        serde_json::from_str::<serde_json::Value>(raw).map_err(|e| e.to_string())
    }

    fn parse_json_typed_strict<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, String> {
        let v = Self::parse_json_object_strict(raw)?;
        serde_json::from_value::<T>(v).map_err(|e| e.to_string())
    }

    fn sanitize_impl_spec_value(
        mut v: serde_json::Value,
        track: TrackKind,
    ) -> (serde_json::Value, Vec<String>) {
        let mut stripped: Vec<String> = Vec::new();
        let Some(obj) = v.as_object_mut() else {
            return (v, stripped);
        };
        let allowed: HashSet<&'static str> = if track == TrackKind::Cleanse {
            [
                "spec_version",
                "row_preserving",
                "output_fields",
                "prohibited_ops",
            ]
            .into_iter()
            .collect()
        } else {
            [
                "spec_version",
                "grain",
                "inputs",
                "joins",
                "metrics",
                "output_fields",
                "assumptions",
            ]
            .into_iter()
            .collect()
        };
        let keys: Vec<String> = obj.keys().cloned().collect();
        for k in keys {
            if !allowed.contains(k.as_str()) {
                obj.remove(&k);
                stripped.push(k);
            }
        }
        (v, stripped)
    }

    fn parse_impl_spec_value_with_sanitize<T: serde::de::DeserializeOwned>(
        v: serde_json::Value,
        track: TrackKind,
    ) -> Result<(T, Vec<String>), String> {
        let (sv, stripped) = Self::sanitize_impl_spec_value(v, track);
        Self::validate_output_field_kind_contract(&sv)?;
        let spec = serde_json::from_value::<T>(sv).map_err(|e| e.to_string())?;
        Ok((spec, stripped))
    }

    fn validate_output_field_kind_contract(v: &serde_json::Value) -> Result<(), String> {
        let Some(obj) = v.as_object() else {
            return Ok(());
        };
        let Some(output_fields) = obj.get("output_fields") else {
            return Ok(());
        };
        let Some(arr) = output_fields.as_array() else {
            return Ok(());
        };
        let mut errs: Vec<String> = Vec::new();
        for (idx, it) in arr.iter().enumerate() {
            let Some(kv) = it.get("kind") else {
                continue;
            };
            let Some(ks) = kv.as_str() else {
                errs.push(format!("output_fields[{idx}].kind must be a string"));
                continue;
            };
            if !matches!(ks, "raw" | "clean" | "derived" | "quality_flag") {
                errs.push(format!("output_fields[{idx}].kind='{}' is invalid", ks));
            }
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{}; allowed kind values: raw, clean, derived, quality_flag",
                errs.join("; ")
            ))
        }
    }

    fn push_snapshot_array_event(
        snapshot: &mut serde_json::Value,
        key: &str,
        event: serde_json::Value,
        max_len: usize,
    ) {
        if snapshot.is_null() {
            *snapshot = serde_json::json!({});
        }
        if let Some(obj) = snapshot.as_object_mut() {
            let arr = obj
                .entry(key.to_string())
                .or_insert_with(|| serde_json::Value::Array(vec![]));
            if let Some(items) = arr.as_array_mut() {
                items.push(event);
                while items.len() > max_len {
                    items.remove(0);
                }
            }
        }
    }

}

#[async_trait]
impl Suite for DataEngineerSuite {
    fn id(&self) -> &'static str {
        "data_engineer"
    }

    fn label(&self) -> &'static str {
        "Data Engineer"
    }

    fn supported_agent_types(&self) -> Vec<String> {
        vec![
            "ask".to_string(),
            "agent".to_string(),
            "review".to_string(),
        ]
    }

    fn default_agent_type(&self) -> &'static str {
        "ask"
    }

    fn phase_order(&self, agent_type: &str) -> Vec<String> {
        // Only expose phases for agent-mode; other modes are single-pass.
        if AgentMode::parse(agent_type).ok() != Some(AgentMode::Agent) {
            return Vec::new();
        }
        use crate::data_engineer::control_flow::Phase;
        vec![
            Phase::Preflight.as_str(),
            Phase::CleansePlan.as_str(),
            Phase::CleanseAuthor.as_str(),
            Phase::CleanseValidate.as_str(),
            Phase::CleanseReview.as_str(),
            Phase::ModelPlan.as_str(),
            Phase::ModelAuthor.as_str(),
            Phase::ModelValidate.as_str(),
            Phase::ModelReview.as_str(),
            Phase::PublishAwaitApproval.as_str(),
            Phase::Publish.as_str(),
            Phase::PostPublishReview.as_str(),
            Phase::Done.as_str(),
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect()
    }

    async fn load_ws_plans(
        &self,
        thread_id: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<serde_json::Value>, String> {
        Ok(ws_plans::load_latest_plans_ws(ctx, thread_id).await)
    }

    async fn handle_new(
        &self,
        thread_id: &str,
        question: &str,
        agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        self.dispatch_agent(thread_id, question, agent_type, ctx).await
    }

    async fn handle_open(
        &self,
        thread_id: &str,
        question: &str,
        agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        self.dispatch_agent(thread_id, question, agent_type, ctx).await
    }

    async fn handle_user(
        &self,
        thread_id: &str,
        text: &str,
        agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        self.dispatch_agent(thread_id, text, agent_type, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;

    #[derive(Clone)]
    struct MockWarehouseOk {
        ok_fqns: std::collections::HashSet<String>,
    }

    #[async_trait]
    impl crate::data_engineer::providers::QueryProvider for MockWarehouseOk {
        async fn query(&self, _sql: &str) -> Result<crate::data_engineer::providers::QueryResult, String> {
            Ok(crate::data_engineer::providers::QueryResult {
                header: vec![],
                rows: vec![],
                meta: None,
            })
        }

        async fn schema(&self, dataset_fqn: &str) -> Result<Vec<(String, String)>, String> {
            if self.ok_fqns.contains(dataset_fqn) {
                Ok(vec![("x".to_string(), "string".to_string())])
            } else {
                Err("not found".to_string())
            }
        }

        async fn sample(
            &self,
            _dataset_fqn: &str,
            _limit: usize,
        ) -> Result<Vec<Vec<String>>, String> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl crate::data_engineer::providers::DatasetCatalogProvider for MockWarehouseOk {
        async fn list_datasets(&self) -> Result<Vec<crate::data_engineer::providers::DatasetId>, String> {
            Ok(vec![])
        }

        async fn get_dataset_schema(
            &self,
            dataset: &crate::data_engineer::providers::DatasetId,
        ) -> Result<Vec<(String, String)>, String> {
            let fqn = dataset.fqn();
            crate::data_engineer::providers::QueryProvider::schema(self, fqn.as_str()).await
        }

        async fn get_dataset_stats(
            &self,
            _dataset: &crate::data_engineer::providers::DatasetId,
            _max_fields: usize,
        ) -> Result<
            (
                crate::data_engineer::providers::DatasetFieldStats,
                crate::data_engineer::providers::DatasetStats,
            ),
            String,
        > {
            Err("not used".to_string())
        }
    }

    impl crate::data_engineer::providers::WarehouseNaming for MockWarehouseOk {
        fn kind(&self) -> crate::data_engineer::de_config::WarehouseKind {
            crate::data_engineer::de_config::WarehouseKind::default()
        }

        fn parse_dataset_fqn(
            &self,
            dataset_fqn: &str,
        ) -> Result<crate::data_engineer::providers::DatasetId, String> {
            let parts: Vec<&str> = dataset_fqn.split('.').collect();
            if parts.len() != 3 {
                return Err("expected <catalog>.<schema>.<table>".to_string());
            }
            Ok(crate::data_engineer::providers::DatasetId {
                catalog: parts[0].to_string(),
                database: parts[1].to_string(),
                table: parts[2].to_string(),
            })
        }

        fn quote_ident(&self, ident: &str) -> String {
            format!("\"{}\"", ident.replace('"', "\"\""))
        }
    }

    struct MockQuery;

    #[async_trait]
    impl crate::data_engineer::providers::QueryProvider for MockQuery {
        async fn query(&self, _sql: &str) -> Result<crate::data_engineer::providers::QueryResult, String> {
            Ok(crate::data_engineer::providers::QueryResult {
                header: vec![],
                rows: vec![],
                meta: None,
            })
        }
        async fn schema(&self, _dataset_fqn: &str) -> Result<Vec<(String, String)>, String> {
            Ok(vec![])
        }
        async fn sample(
            &self,
            _dataset_fqn: &str,
            _limit: usize,
        ) -> Result<Vec<Vec<String>>, String> {
            Ok(vec![])
        }
    }

    #[test]
    fn select_high_value_model_candidates_uses_score_threshold_not_fixed_count() {
        let candidates: Vec<crate::data_engineer::plan_schema::ModelPlanCandidateV1> = (0..10)
            .map(|i| crate::data_engineer::plan_schema::ModelPlanCandidateV1 {
                name: format!("m_{i}"),
                insight: "x".to_string(),
                observation: "y".to_string(),
                value_score: 95 - (i as i32 * 5),
            })
            .collect();
        let selected = DataEngineerSuite::select_high_value_model_candidates(&candidates);
        assert_eq!(selected.len(), 6, "default min score 70 should keep 6");
        assert_eq!(selected[0].name, "m_0");
        assert_eq!(selected[5].name, "m_5");
    }

    #[test]
    fn compile_model_candidates_plan_builds_batches_from_selected_threshold() {
        let cands = crate::data_engineer::plan_schema::ModelPlanCandidatesV1 {
            candidates: (0..13)
                .map(|i| crate::data_engineer::plan_schema::ModelPlanCandidateV1 {
                    name: format!("m_{i}"),
                    insight: "high value".to_string(),
                    observation: "grounded".to_string(),
                    value_score: 100 - (i as i32 * 5),
                })
                .collect(),
        };
        let plan = DataEngineerSuite::compile_model_candidates_plan(&cands);
        let tasks = plan.tasks;
        let batches = plan.batches;
        assert_eq!(tasks.len(), 7, "default min score 70 should keep 7");
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 5);
        assert_eq!(batches[1].len(), 2);
    }

    #[test]
    fn parse_impl_spec_with_sanitize_strips_unknown_cleanse_keys() {
        let raw = serde_json::json!({
            "spec_version": 1,
            "row_preserving": true,
            "output_fields": [],
            "prohibited_ops": [],
            "batch_id": "x",
            "data_quality": {"checks":[]}
        });
        let (spec, stripped) = DataEngineerSuite::parse_impl_spec_value_with_sanitize::<
            crate::data_engineer::plan::CleanseImplementationSpec,
        >(raw, TrackKind::Cleanse)
        .expect("cleanse spec should parse after sanitize");
        assert_eq!(spec.spec_version, 1);
        assert!(stripped.iter().any(|k| k == "batch_id"));
        assert!(stripped.iter().any(|k| k == "data_quality"));
    }

    #[test]
    fn parse_impl_spec_with_sanitize_strips_unknown_model_keys() {
        let raw = serde_json::json!({
            "spec_version": 1,
            "grain": "1 row per id",
            "inputs": [],
            "joins": [],
            "metrics": [],
            "output_fields": [],
            "assumptions": [],
            "dependencies": ["x"],
            "batch_id": "b1"
        });
        let (spec, stripped) = DataEngineerSuite::parse_impl_spec_value_with_sanitize::<
            crate::data_engineer::plan::ModelImplementationSpec,
        >(raw, TrackKind::Model)
        .expect("model spec should parse after sanitize");
        assert_eq!(spec.spec_version, 1);
        assert!(stripped.iter().any(|k| k == "dependencies"));
        assert!(stripped.iter().any(|k| k == "batch_id"));
    }

    #[tokio::test]
    async fn review_registry_is_read_only() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));

        let reg = DataEngineerSuite::build_tools(AgentMode::Review, &sctx)
            .expect("build_tools(review) should succeed");
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);

        // Not allowed in review
        assert!(reg
            .call("approve_and_save_artifact", serde_json::json!({}), &actx)
            .await
            .is_err());
        assert!(reg
            .call(
                "approve_and_save_artifact_batch",
                serde_json::json!({}),
                &actx
            )
            .await
            .is_err());
        assert!(reg
            .call("dbt_validate", serde_json::json!({}), &actx)
            .await
            .is_err());
        assert!(reg
            .call("publish_dbt_to_provider", serde_json::json!({}), &actx)
            .await
            .is_err());
        assert!(reg
            .call("staging_model", serde_json::json!({}), &actx)
            .await
            .is_err());
        assert!(reg
            .call("catalog_note", serde_json::json!({}), &actx)
            .await
            .is_err());
        assert!(reg
            .call("ask_user", serde_json::json!({}), &actx)
            .await
            .is_err());
        assert!(reg
            .call("ask_approval", serde_json::json!({}), &actx)
            .await
            .is_err());

        // Also exclude arbitrary SQL execution in review mode.
        assert!(reg
            .call("run_sql", serde_json::json!({"sql":"SELECT 1"}), &actx)
            .await
            .is_err());

        // Allowed in review
        let obs = reg
            .call(
                "artifacts",
                serde_json::json!({"op":"list","limit":5}),
                &actx,
            )
            .await
            .expect("artifacts should be available");
        assert_eq!(obs.get("ok").and_then(|v| v.as_bool()), Some(true));
    }

    #[tokio::test]
    async fn agent_authoring_hard_mutation_phase_locks_tools() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);

        let guard = crate::data_engineer::control_flow::DerivedGuardState {
            last_validate_failed: true,
            mutated_since_fail: false,
            patched_since_fail: false,
            mutation_failures_since_validate: 0,
            probe_required: false,
            probe_satisfied: false,
        };
        let (reg, card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::ModelAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::Unconstrained,
            None,
            false,
        )
        .expect("build_tools_for_phase should succeed");

        // Tool card should advertise direct overwrite patching (not apply_next_* tools).
        assert!(card.contains("patch_text"));
        assert!(!card.contains("apply_next_model_batch"));

        // run_sql should not be available in hard mutation-only mode
        assert!(reg
            .call("run_sql", serde_json::json!({"sql":"SELECT 1"}), &actx)
            .await
            .is_err());

        // file get should be blocked (put-only wrapper)
        assert!(reg
            .call(
                "file",
                serde_json::json!({"op":"get","path":"dbt_project.yml"}),
                &actx
            )
            .await
            .is_err());

        // apply_next_* tools should not be available in hard mutation-only mode
        let err = reg
            .call("apply_next_model_batch", serde_json::json!({}), &actx)
            .await
            .unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    #[tokio::test]
    async fn hard_mutation_run_sql_records_probe_attempts_to_execution_state() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("probe-thread", &sctx);
        let store = actx.thread_store.as_ref().expect("thread_store");

        let mut st = crate::data_engineer::progress_controller::ExecutionState::new();
        st.telemetry.last_validate = Some(
            crate::data_engineer::progress_controller::LastValidateState {
                ok: Some(false),
                ..crate::data_engineer::progress_controller::LastValidateState::default()
            },
        );
        st.repair.repair_mode =
            crate::data_engineer::progress_controller::RepairModeState::SqlTarget(
                crate::data_engineer::progress_controller::SqlTargetRepairMode {
                    target_path: crate::data_engineer::progress_controller::SqlModelPath::parse("models/staging/stg_probe.sql".to_string()).expect("valid sql model path"),
                    core: crate::data_engineer::progress_controller::RepairModeCore {
                        ladder_step: crate::data_engineer::progress_controller::RepairLadderStep::PatchTarget,
                        attempt_count: 0,
                        repair_started_mutation_epoch: None,
                        consecutive_noop_patches: 0,
                    },
                },
            );
        st.telemetry.probe.required = true;
        st.save(store, "probe-thread").await.expect("save state");

        let guard = crate::data_engineer::control_flow::DerivedGuardState {
            last_validate_failed: true,
            mutated_since_fail: false,
            patched_since_fail: false,
            mutation_failures_since_validate: 0,
            probe_required: false,
            probe_satisfied: false,
        };
        let (reg, _) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::ModelAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::Unconstrained,
            None,
            false,
        )
        .expect("build_tools_for_phase should succeed");

        let err = reg
            .call(
                "run_sql",
                serde_json::json!({"sql":"SELECT count(*) FROM some_table"}),
                &actx,
            )
            .await
            .expect_err("run_sql should not be exposed in hard mutation mode");
        assert!(err.contains("unknown tool"));

        let updated = crate::data_engineer::progress_controller::ExecutionState::load(store, "probe-thread")
            .await
            .expect("state should load");
        assert_eq!(updated.telemetry.probe.attempts_total, 0);
        assert_eq!(updated.telemetry.probe.meaningful_attempts, 0);
    }

    #[tokio::test]
    async fn hard_mutation_run_sql_is_blocked_after_probe_exhaustion() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("probe-exhausted-thread", &sctx);
        let store = actx.thread_store.as_ref().expect("thread_store");

        let mut st = crate::data_engineer::progress_controller::ExecutionState::new();
        st.telemetry.last_validate = Some(
            crate::data_engineer::progress_controller::LastValidateState {
                ok: Some(false),
                ..crate::data_engineer::progress_controller::LastValidateState::default()
            },
        );
        st.repair.repair_mode =
            crate::data_engineer::progress_controller::RepairModeState::SqlTarget(
                crate::data_engineer::progress_controller::SqlTargetRepairMode {
                    target_path: crate::data_engineer::progress_controller::SqlModelPath::parse("models/staging/stg_probe.sql".to_string()).expect("valid sql model path"),
                    core: crate::data_engineer::progress_controller::RepairModeCore {
                        ladder_step: crate::data_engineer::progress_controller::RepairLadderStep::PatchTarget,
                        attempt_count: 0,
                        repair_started_mutation_epoch: None,
                        consecutive_noop_patches: 0,
                    },
                },
            );
        st.telemetry.probe.required = true;
        let sig = crate::data_engineer::progress_controller::ProbeSignature::from_run_sql(
            "select * from t limit 10",
            &serde_json::json!({"ok":true}),
        );
        let _ = st.note_probe_attempt("select * from t limit 10", true, sig.clone());
        let _ = st.note_probe_attempt("select * from t limit 10", true, sig.clone());
        let _ = st.note_probe_attempt("select * from t limit 10", true, sig.clone());
        let _ = st.note_probe_attempt("select * from t limit 10", true, sig);
        st.save(store, "probe-exhausted-thread")
            .await
            .expect("save state");

        let guard = crate::data_engineer::control_flow::DerivedGuardState {
            last_validate_failed: true,
            mutated_since_fail: false,
            patched_since_fail: false,
            mutation_failures_since_validate: 0,
            probe_required: false,
            probe_satisfied: false,
        };
        let (reg, _) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::ModelAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::Unconstrained,
            None,
            false,
        )
        .expect("build_tools_for_phase should succeed");

        let err = reg
            .call(
                "run_sql",
                serde_json::json!({"sql":"SELECT count(*) FROM some_table"}),
                &actx,
            )
            .await
            .expect_err("run_sql should be blocked after exhaustion");
        assert!(err.contains("unknown tool"));
    }

    #[tokio::test]
    async fn hard_mutation_mode_exposes_batch_tool_from_plan_state() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));

        let guard = crate::data_engineer::control_flow::DerivedGuardState {
            last_validate_failed: true,
            mutated_since_fail: false,
            patched_since_fail: false,
            mutation_failures_since_validate: 0,
            probe_required: false,
            probe_satisfied: false,
        };

        let (_reg, card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::CleanseAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::CleanseSqlDatasetIds(vec![
                "AwsDataCatalog.db.t1".to_string(),
            ]),
            None,
            false,
        )
        .expect("build_tools_for_phase should succeed");

        assert!(card.contains("patch_text"));
        assert!(card.contains("apply_next_cleanse_batch"));
    }

    #[tokio::test]
    async fn hard_mutation_single_target_hides_schema_batch_tools() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);

        let guard = crate::data_engineer::control_flow::DerivedGuardState {
            last_validate_failed: true,
            mutated_since_fail: false,
            patched_since_fail: false,
            mutation_failures_since_validate: 0,
            probe_required: false,
            probe_satisfied: false,
        };

        let (reg, card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::CleanseAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::CleanseSchemaDatasetIds(vec![
                "AwsDataCatalog.db.t1".to_string(),
            ]),
            Some("models/staging/stg_test_raw_raw_order_items.sql".to_string()),
            false,
        )
        .expect("build_tools_for_phase should succeed");

        assert!(
            !card.contains("apply_next_cleanse_schema_batch"),
            "single-target SQL repair mode must not expose schema batch tools"
        );
        let err = reg
            .call(
                "apply_next_cleanse_schema_batch",
                serde_json::json!({}),
                &actx,
            )
            .await
            .unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    #[tokio::test]
    async fn hard_mutation_mode_single_target_repair_rejects_other_paths() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);

        let guard = crate::data_engineer::control_flow::DerivedGuardState {
            last_validate_failed: true,
            mutated_since_fail: false,
            patched_since_fail: false,
            mutation_failures_since_validate: 0,
            probe_required: false,
            probe_satisfied: false,
        };

        let (reg, _card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::ModelAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::Unconstrained,
            Some("models/marts/fct_orders.sql".to_string()),
            false,
        )
        .expect("build_tools_for_phase should succeed");

        // Seed valid hard-repair execution state for deterministic single-target tool calls.
        if let Some(store) = actx.thread_store.as_ref() {
            let mut seeded = crate::data_engineer::progress_controller::ExecutionState::new();
            seeded.telemetry.last_validate = Some(
                crate::data_engineer::progress_controller::LastValidateState {
                    ok: Some(false),
                    ..crate::data_engineer::progress_controller::LastValidateState::default()
                },
            );
            seeded.repair.repair_mode = crate::data_engineer::progress_controller::RepairModeState::SqlTarget(
                crate::data_engineer::progress_controller::SqlTargetRepairMode {
                    target_path: crate::data_engineer::progress_controller::SqlModelPath::parse("models/marts/fct_orders.sql".to_string()).expect("valid sql model path"),
                    core: crate::data_engineer::progress_controller::RepairModeCore {
                        ladder_step: crate::data_engineer::progress_controller::RepairLadderStep::PatchTarget,
                        attempt_count: 0,
                        repair_started_mutation_epoch: None,
                        consecutive_noop_patches: 0,
                    },
                },
            );
            seeded
                .save(store, "t")
                .await
                .expect("seed hard repair state");
        }

        let err = reg
            .call(
                "file",
                serde_json::json!({
                    "op":"patch",
                    "path":"models/marts/fct_customers.sql",
                    "patch_text":"@@\n- select 1 as id\n+ select 1 as id\n"
                }),
                &actx,
            )
            .await
            .unwrap_err();
        assert!(err.contains("single-target repair mode violation"));
    }

    #[tokio::test]
    async fn hard_mutation_mode_single_target_patch_target_rejects_rm() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);

        let guard = crate::data_engineer::control_flow::DerivedGuardState {
            last_validate_failed: true,
            mutated_since_fail: false,
            patched_since_fail: false,
            mutation_failures_since_validate: 0,
            probe_required: false,
            probe_satisfied: false,
        };

        let (reg, _card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::ModelAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::Unconstrained,
            Some("models/marts/fct_orders.sql".to_string()),
            false,
        )
        .expect("build_tools_for_phase should succeed");

        if let Some(store) = actx.thread_store.as_ref() {
            let mut seeded = crate::data_engineer::progress_controller::ExecutionState::new();
            seeded.telemetry.last_validate = Some(
                crate::data_engineer::progress_controller::LastValidateState {
                    ok: Some(false),
                    ..crate::data_engineer::progress_controller::LastValidateState::default()
                },
            );
            seeded.repair.repair_mode = crate::data_engineer::progress_controller::RepairModeState::SqlTarget(
                crate::data_engineer::progress_controller::SqlTargetRepairMode {
                    target_path: crate::data_engineer::progress_controller::SqlModelPath::parse("models/marts/fct_orders.sql".to_string()).expect("valid sql model path"),
                    core: crate::data_engineer::progress_controller::RepairModeCore {
                        ladder_step: crate::data_engineer::progress_controller::RepairLadderStep::PatchTarget,
                        attempt_count: 0,
                        repair_started_mutation_epoch: None,
                        consecutive_noop_patches: 0,
                    },
                },
            );
            seeded
                .save(store, "t")
                .await
                .expect("seed patch-target hard repair state");
        }

        let err_target = reg
            .call(
                "file",
                serde_json::json!({"op":"rm","path":"models/marts/fct_orders.sql"}),
                &actx,
            )
            .await
            .unwrap_err();
        assert!(err_target.contains("patch_target requires op='patch'"));
    }

    #[tokio::test]
    async fn hard_mutation_mode_single_target_replace_contents_rejects_rm() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);

        let guard = crate::data_engineer::control_flow::DerivedGuardState {
            last_validate_failed: true,
            mutated_since_fail: false,
            patched_since_fail: false,
            mutation_failures_since_validate: 0,
            probe_required: false,
            probe_satisfied: false,
        };

        let (reg, _card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::ModelAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::Unconstrained,
            Some("models/marts/fct_orders.sql".to_string()),
            false,
        )
        .expect("build_tools_for_phase should succeed");

        if let Some(store) = actx.thread_store.as_ref() {
            let mut seeded = crate::data_engineer::progress_controller::ExecutionState::new();
            seeded.telemetry.last_validate = Some(
                crate::data_engineer::progress_controller::LastValidateState {
                    ok: Some(false),
                    ..crate::data_engineer::progress_controller::LastValidateState::default()
                },
            );
            seeded.repair.repair_mode = crate::data_engineer::progress_controller::RepairModeState::SqlTarget(
                crate::data_engineer::progress_controller::SqlTargetRepairMode {
                    target_path: crate::data_engineer::progress_controller::SqlModelPath::parse("models/marts/fct_orders.sql".to_string()).expect("valid sql model path"),
                    core: crate::data_engineer::progress_controller::RepairModeCore {
                        ladder_step: crate::data_engineer::progress_controller::RepairLadderStep::ReplaceContents,
                        attempt_count: 0,
                        repair_started_mutation_epoch: None,
                        consecutive_noop_patches: 0,
                    },
                },
            );
            seeded
                .save(store, "t")
                .await
                .expect("seed replace-contents hard repair state");
        }

        let err_off_target = reg
            .call(
                "file",
                serde_json::json!({"op":"rm","path":"models/marts/fct_other.sql"}),
                &actx,
            )
            .await
            .unwrap_err();
        assert!(err_off_target.contains("single-target repair mode violation"));

        let target_err = reg
            .call(
                "file",
                serde_json::json!({"op":"rm","path":"models/marts/fct_orders.sql"}),
                &actx,
            )
            .await
            .unwrap_err();
        assert!(
            target_err.contains("replace_contents requires op='patch'"),
            "replace_contents step should reject rm even on target path"
        );
    }

    #[tokio::test]
    async fn hard_mutation_mode_single_target_fs_op_rejects_patch_allows_rm() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);

        let guard = crate::data_engineer::control_flow::DerivedGuardState {
            last_validate_failed: true,
            mutated_since_fail: false,
            patched_since_fail: false,
            mutation_failures_since_validate: 0,
            probe_required: false,
            probe_satisfied: false,
        };

        let (reg, _card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::ModelAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::Unconstrained,
            Some("models/marts/fct_orders.sql".to_string()),
            false,
        )
        .expect("build_tools_for_phase should succeed");

        if let Some(store) = actx.thread_store.as_ref() {
            let mut seeded = crate::data_engineer::progress_controller::ExecutionState::new();
            seeded.telemetry.last_validate = Some(
                crate::data_engineer::progress_controller::LastValidateState {
                    ok: Some(false),
                    ..crate::data_engineer::progress_controller::LastValidateState::default()
                },
            );
            seeded.repair.repair_mode = crate::data_engineer::progress_controller::RepairModeState::SqlTarget(
                crate::data_engineer::progress_controller::SqlTargetRepairMode {
                    target_path: crate::data_engineer::progress_controller::SqlModelPath::parse("models/marts/fct_orders.sql".to_string()).expect("valid sql model path"),
                    core: crate::data_engineer::progress_controller::RepairModeCore {
                        ladder_step: crate::data_engineer::progress_controller::RepairLadderStep::FsOp,
                        attempt_count: 2,
                        repair_started_mutation_epoch: None,
                        consecutive_noop_patches: 0,
                    },
                },
            );
            seeded
                .save(store, "t")
                .await
                .expect("seed fs-op hard repair state");
        }

        let patch_err = reg
            .call(
                "file",
                serde_json::json!({
                    "op":"patch",
                    "path":"models/marts/fct_orders.sql",
                    "patch_text":"@@\n- select 1 as id\n+ select 2 as id\n"
                }),
                &actx,
            )
            .await
            .unwrap_err();
        assert!(patch_err.contains("fs_op requires op='rm' or op='mv'"));

        let rm_result = reg
            .call(
                "file",
                serde_json::json!({"op":"rm","path":"models/marts/fct_orders.sql"}),
                &actx,
            )
            .await;
        assert!(rm_result.is_ok(), "fs_op should allow rm on target path");
    }

    #[tokio::test]
    async fn agent_phase_tool_card_and_registry_never_expose_ask_user() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);
        let guard = crate::data_engineer::control_flow::DerivedGuardState::default();

        let (reg, card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::CleansePlan,
            &guard,
            true,
            &sctx,
            &super::PlanState::Unconstrained,
            None,
            false,
        )
        .expect("build_tools_for_phase should succeed");

        assert!(!card.contains("ask_user"));
        let err = reg
            .call("ask_user", serde_json::json!({"prompt":"x"}), &actx)
            .await
            .unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    #[tokio::test]
    async fn run_agent_is_non_interactive_on_missing_providers() {
        let sctx = SuiteCtx::default();
        let err = DataEngineerSuite::run_agent("thread-missing-providers", "go", &sctx)
            .await
            .expect_err("agent mode must hard-fail instead of returning an interactive prompt");
        assert!(
            err.contains("warehouse provider configured")
                || err.contains("dbt provider configured")
                || err.contains("catalog bootstrap metadata gate failed")
        );
    }

    #[test]
    fn non_interactive_contract_rejects_await_user_for_agent_type() {
        let frames = vec![FlowFrame::Interrupt {
            kind: FlowKind::new("await_user"),
            prompt: "x".to_string(),
        }];
        let err = DataEngineerSuite::enforce_non_interactive_contract(AgentMode::Agent, frames)
            .expect_err("agent type must reject await_user interrupt");
        assert!(err.contains("agent_mode_await_user_forbidden"));
    }

    #[test]
    fn non_interactive_contract_allows_await_user_for_non_agent_when_not_headless() {
        let frames = vec![FlowFrame::Interrupt {
            kind: FlowKind::new("await_user"),
            prompt: "x".to_string(),
        }];
        let out = DataEngineerSuite::enforce_non_interactive_contract(AgentMode::Review, frames)
            .expect("non-agent should allow await_user interrupt when not headless");
        assert!(matches!(out.first(), Some(FlowFrame::Interrupt { .. })));
    }

    #[tokio::test]
    async fn plan_batched_staging_model_is_not_exposed_to_agent() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);

        let guard = crate::data_engineer::control_flow::DerivedGuardState::default();
        let (reg, _card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::CleanseAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::CleanseSqlDatasetIds(vec![
                "AwsDataCatalog.db.t1".to_string(),
            ]),
            None,
            false,
        )
        .expect("build_tools_for_phase should succeed");

        let err = reg
            .call(
                "staging_model",
                serde_json::json!({"dataset_ids":["AwsDataCatalog.db.t2"]}),
                &actx,
            )
            .await
            .unwrap_err();
        assert!(err.contains("unknown tool"));

        // Deterministic executor tool should exist (even if it fails due to missing plan in this test ctx).
        let err2 = reg
            .call("apply_next_cleanse_batch", serde_json::json!({}), &actx)
            .await
            .unwrap_err();
        assert!(err2.contains("no active cleanse plan"));
    }

    #[tokio::test]
    async fn plan_batched_cleanse_schema_mode_exposes_only_schema_batch_tool() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);
        let guard = crate::data_engineer::control_flow::DerivedGuardState::default();
        let (reg, card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::CleanseAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::CleanseSchemaDatasetIds(vec![
                "AwsDataCatalog.db.t1".to_string(),
            ]),
            None,
            false,
        )
        .expect("build_tools_for_phase should succeed");
        assert!(card.contains("apply_next_cleanse_schema_batch"));
        assert!(
            !card.contains("- apply_next_cleanse_batch(args:{instructions?:string})"),
            "sql batch tool must not be exposed in schema-next-action mode"
        );
        let err = reg
            .call("apply_next_cleanse_batch", serde_json::json!({}), &actx)
            .await
            .unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    #[tokio::test]
    async fn plan_batched_gold_model_is_not_exposed_to_agent() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);

        let guard = crate::data_engineer::control_flow::DerivedGuardState::default();
        let (reg, _card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::ModelAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::ModelSqlItemNames(vec![
                "fct_orders".to_string()
            ]),
            None,
            false,
        )
        .expect("build_tools_for_phase should succeed");

        let err = reg
            .call(
                "gold_model",
                serde_json::json!({"items":[{"name":"dim_users","inputs":["stg_x"]}]}),
                &actx,
            )
            .await
            .unwrap_err();
        assert!(err.contains("unknown tool"));

        let err2 = reg
            .call("apply_next_model_batch", serde_json::json!({}), &actx)
            .await
            .unwrap_err();
        assert!(err2.contains("no active model plan"));
    }

    #[tokio::test]
    async fn plan_batched_model_schema_mode_exposes_only_schema_batch_tool() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);
        let guard = crate::data_engineer::control_flow::DerivedGuardState::default();
        let (reg, card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::ModelAuthor,
            &guard,
            true,
            &sctx,
            &super::PlanState::ModelSchemaItemNames(vec![
                "fct_orders".to_string(),
            ]),
            None,
            false,
        )
        .expect("build_tools_for_phase should succeed");
        assert!(card.contains("apply_next_model_schema_batch"));
        assert!(
            !card.contains("- apply_next_model_batch(args:{instructions?:string})"),
            "sql batch tool must not be exposed in schema-next-action mode"
        );
        let err = reg
            .call("apply_next_model_batch", serde_json::json!({}), &actx)
            .await
            .unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    #[test]
    fn review_question_includes_prior_review_and_mutation_diff_when_available() {
        use crate::data_engineer::control_flow::Phase;
        use crate::data_engineer::progress_controller::{ExecutionState, LastMutationSummary};

        let prior_review_answer = "Please add tests.";
        let mut st = ExecutionState::new();
        st.phase.phase_reason_code = Some(crate::data_engineer::domain_types::PhaseReasonCode::ReviewPatchImpl);
        st.phase.phase_reason_detail = Some(serde_json::json!({
            "review_phase":"cleanse_review",
            "meta": {"decision":"patch_impl", "dataset_ids": ["x"], "tier":"silver"},
            "answer": prior_review_answer
        }));
        st.telemetry.last_mutation_summary = Some(LastMutationSummary {
            op: crate::data_engineer::progress_controller::MutationOp::Patch,
            affected_paths: vec!["models/staging/stg_test_raw_raw_orders.sql".to_string()],
            select_terms: vec!["placed_at_ts".to_string()],
            ts: Some("t".to_string()),
        });

        let q = DataEngineerSuite::build_review_question_with_context(
            "orig goal",
            Phase::CleanseReview,
            &st,
        );
        assert!(
            q.contains("Review context"),
            "should include context header"
        );
        assert!(
            q.contains("Previous review decision"),
            "should include prior review block"
        );
        assert!(
            q.contains("Most recent mutation summary"),
            "should include state-based mutation summary"
        );
        assert!(
            q.contains("stg_test_raw_raw_orders.sql"),
            "should include affected path from state"
        );
        assert!(
            q.contains("review_patch_impl"),
            "should include entry reason"
        );
        assert!(
            q.contains("Original goal"),
            "should retain original goal section"
        );
    }

    #[test]
    fn review_question_includes_entry_reason_when_review_started_from_validate_pass() {
        use crate::data_engineer::control_flow::Phase;
        use crate::data_engineer::progress_controller::ExecutionState;

        let mut st = ExecutionState::new();
        st.phase.phase_reason_code = Some(crate::data_engineer::domain_types::PhaseReasonCode::ValidatePassToReview);
        st.phase.phase_reason_detail = Some(serde_json::json!({"dbt_validate_step_idx": 1}));

        let q = DataEngineerSuite::build_review_question_with_context(
            "orig goal",
            Phase::CleanseReview,
            &st,
        );
        assert!(q.contains("validate_pass_to_review"));
        assert!(q.contains("dbt_validate_step_idx"));
    }

    #[test]
    fn patch_impl_intent_requires_mutation_epoch_advance() {
        use crate::data_engineer::control_flow::Phase;
        use crate::data_engineer::progress_controller::{
            ExecutionState, PatchImplIntent,
        };

        let mut st = ExecutionState::new();
        st.repair.mutation_epoch = 4;
        st.repair.pending_patch_impl = Some(PatchImplIntent {
            phase: Phase::ModelAuthor,
            entry_mutation_epoch: 4,
        });
        assert!(crate::data_engineer::phase_gate::patch_impl_intent_unsatisfied(
            &st,
            Phase::ModelAuthor
        ));
        st.repair.mutation_epoch = 5;
        assert!(!crate::data_engineer::phase_gate::patch_impl_intent_unsatisfied(
            &st,
            Phase::ModelAuthor
        ));
    }

    #[test]
    fn derive_single_target_repair_path_prefers_execution_state_target() {
        use crate::data_engineer::progress_controller::{ExecutionState, FailedModelRef};
        let mut st = ExecutionState::new();
        st.repair.repair_mode =
            crate::data_engineer::progress_controller::RepairModeState::SqlTarget(
                crate::data_engineer::progress_controller::SqlTargetRepairMode {
                    target_path: crate::data_engineer::progress_controller::SqlModelPath::parse("models/staging/stg_orders.sql".to_string()).expect("valid sql model path"),
                    core: crate::data_engineer::progress_controller::RepairModeCore {
                        ladder_step: crate::data_engineer::progress_controller::RepairLadderStep::PatchTarget,
                        attempt_count: 0,
                        repair_started_mutation_epoch: None,
                        consecutive_noop_patches: 0,
                    },
                },
            );
        let _failed = vec![FailedModelRef {
            name: "stg_other".to_string(),
            file: "models/staging/stg_other.sql".to_string(),
            ..Default::default()
        }];
        let got = crate::data_engineer::phase_gate::derive_single_target_repair_path(&st);
        assert_eq!(got, Some("models/staging/stg_orders.sql".to_string()));
    }

    #[test]
    fn derive_single_target_repair_path_does_not_fallback_to_failed_model_file() {
        use crate::data_engineer::progress_controller::{ExecutionState, FailedModelRef};
        let st = ExecutionState::new();
        let failed = vec![FailedModelRef {
            name: "stg_orders".to_string(),
            file: "models/staging/stg_orders.sql".to_string(),
            ..Default::default()
        }];
        let _ = failed;
        let got = crate::data_engineer::phase_gate::derive_single_target_repair_path(&st);
        assert_eq!(got, None);
    }

    #[tokio::test]
    async fn authoring_complete_reason_detail_uses_latest_log_state() {
        let sctx = SuiteCtx::default();
        let store = ThreadStore::new(
            sctx.storage.clone(),
            sctx.scope.clone(),
            sctx.keyspace.clone(),
        );
        let tid = "tid_guard_state";

        // Seed failing validate state directly in canonical control state.
        let mut state = crate::data_engineer::progress_controller::ExecutionState::new();
        state.telemetry.last_validate = Some(
            crate::data_engineer::progress_controller::LastValidateState {
                ok: Some(false),
                ..crate::data_engineer::progress_controller::LastValidateState::default()
            },
        );
        state
            .save(&store, tid)
            .await
            .expect("save failing validate state");

        let before =
            DataEngineerSuite::authoring_complete_reason_detail(&store, tid, true, true).await;
        assert_eq!(
            before
                .get("guard_state")
                .and_then(|v| v.get("patched_since_fail"))
                .and_then(|v| v.as_bool()),
            Some(false)
        );

        // A recorded patch mutation flips patched_since_fail via typed mutation receipt.
        state.repair.repair_mode =
            crate::data_engineer::progress_controller::RepairModeState::SqlTarget(
                crate::data_engineer::progress_controller::SqlTargetRepairMode {
                    target_path: crate::data_engineer::progress_controller::SqlModelPath::parse("models/staging/stg_orders.sql".to_string()).expect("valid sql model path"),
                    core: crate::data_engineer::progress_controller::RepairModeCore {
                        ladder_step: crate::data_engineer::progress_controller::RepairLadderStep::PatchTarget,
                        attempt_count: 0,
                        repair_started_mutation_epoch: None,
                        consecutive_noop_patches: 0,
                    },
                },
            );
        state.set_last_mutation_summary(
            crate::data_engineer::progress_controller::MutationOp::Patch,
            vec!["models/staging/stg_orders.sql".to_string()],
            vec![],
        );
        state
            .save(&store, tid)
            .await
            .expect("save patched state");

        let after =
            DataEngineerSuite::authoring_complete_reason_detail(&store, tid, true, true).await;
        assert_eq!(
            after
                .get("guard_state")
                .and_then(|v| v.get("patched_since_fail"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn output_field_kind_contract_rejects_unknown_variants() {
        let ok = serde_json::json!({
            "output_fields": [{"name":"a","kind":"raw"},{"name":"b","kind":"quality_flag"}]
        });
        assert!(DataEngineerSuite::validate_output_field_kind_contract(&ok).is_ok());
        let bad = serde_json::json!({
            "output_fields": [{"name":"a","kind":"passthrough"}]
        });
        let err = DataEngineerSuite::validate_output_field_kind_contract(&bad)
            .expect_err("expected invalid kind");
        assert!(err.contains("allowed kind values: raw, clean, derived, quality_flag"));
    }

    #[tokio::test]
    async fn model_plan_can_disable_json_file_after_manifest_retry_suppression() {
        let mut sctx = SuiteCtx::default();
        sctx.set_capability(Arc::new(crate::data_engineer::ctx_ext::QueryCap(Arc::new(MockQuery))));
        let actx = DataEngineerSuite::agent_tool_ctx("t", &sctx);
        let guard = crate::data_engineer::control_flow::DerivedGuardState::default();
        let (reg, card) = DataEngineerSuite::build_tools_for_phase(
            crate::data_engineer::control_flow::Phase::ModelPlan,
            &guard,
            true,
            &sctx,
            &super::PlanState::Unconstrained,
            None,
            true,
        )
        .expect("build_tools_for_phase should succeed");
        let err = reg
            .call(
                "json_file",
                serde_json::json!({"op":"query","path":"target/manifest.json","pointer":"/nodes"}),
                &actx,
            )
            .await
            .expect_err("json_file should be disabled in fallback mode");
        assert!(err.contains("unknown tool"));
        assert!(card.contains("json_file is temporarily disabled"));
    }

    #[test]
    fn model_plan_manifest_retry_state_detects_repeated_failures() {
        use crate::data_engineer::progress_controller::{
            classify_manifest_lookup_failure, classify_manifest_lookup_path, ExecutionState,
        };

        let mut st = ExecutionState::new();
        let path_kind = classify_manifest_lookup_path("manifest.json")
            .expect("manifest.json should classify as manifest lookup path");
        let failure_kind = classify_manifest_lookup_failure(&[
            "not found or failed to fetch: NoSuchKey".to_string(),
        ])
        .expect("NoSuchKey failure should classify");

        st.note_manifest_lookup_attempt(path_kind, false, Some(failure_kind));
        st.note_manifest_lookup_attempt(path_kind, false, Some(failure_kind));

        assert!(st.manifest.manifest_lookup.retry_suppressed);
        assert_eq!(st.manifest.manifest_lookup.canonical_success_count, 0);
        assert!(
            st.manifest.manifest_lookup
                .failure_signature
                .as_deref()
                .unwrap_or("")
                .contains("NoSuchKey"),
            "expected NoSuchKey signature"
        );
    }

    #[test]
    fn run_agent_source_enforces_kernel_transition_and_guard_paths() {
        let legacy_transition = ["control_flow::append_phase_with_", "intent", "("].concat();
        let legacy_guard_block = ["ThreadStep::Guard", "Block"].concat();
        let mod_src = include_str!("mod.rs");
        let mod_normalized: String = mod_src.chars().filter(|c| !c.is_whitespace()).collect();
        for (name, normalized) in [("mod.rs", mod_normalized.as_str())] {
            assert!(
                !normalized.contains(&legacy_transition),
                "legacy transition path must not appear in {name}"
            );
            assert!(
                !normalized.contains(&legacy_guard_block),
                "legacy inline GuardBlock construction must not appear in {name}"
            );
            assert!(
                normalized.contains("apply_guard_block("),
                "kernel guard helper should be used in {name}"
            );
        }
        for marker in [
            "execute_preflight_phase(",
            "execute_plan_phase(",
            "execute_author_phase(",
            "execute_validate_phase(",
            "execute_review_phase(",
            "execute_publish_await_approval_phase(",
            "execute_publish_phase(",
        ] {
            assert!(
                mod_src.contains(marker),
                "run loop should route through typed phase executors: missing {marker}"
            );
        }

        for (name, src) in [
            ("phase_preflight.rs", include_str!("phase_preflight.rs")),
            ("phase_plan.rs", include_str!("phase_plan.rs")),
            ("phase_author.rs", include_str!("phase_author.rs")),
            ("phase_validate.rs", include_str!("phase_validate.rs")),
            ("phase_review.rs", include_str!("phase_review.rs")),
            ("phase_publish.rs", include_str!("phase_publish.rs")),
        ] {
            assert!(
                src.contains("commit_phase_decision"),
                "{name} must commit phase changes through commit_phase_decision"
            );
            assert!(
                !src.contains("apply_phase_transition("),
                "{name} must not bypass the phase contract seam"
            );
        }
    }

    #[test]
    fn control_state_thread_log_read_guardrails_are_enforced_in_rust_tests() {
        let forbidden = ["thread_store.get(", "store.get(thread_id)"];
        let sources = [
            (
                "control_flow.rs",
                include_str!("control_flow.rs"),
            ),
            (
                "progress_controller.rs",
                include_str!("progress_controller.rs"),
            ),
            (
                "transition_dispatcher.rs",
                include_str!("transition_dispatcher.rs"),
            ),
        ];
        for (name, src) in sources {
            for token in forbidden {
                assert!(
                    !src.contains(token),
                    "forbidden thread-log read token '{}' found in {}",
                    token,
                    name
                );
            }
        }
    }
}
