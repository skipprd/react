use super::*;
use crate::tools::sql_run::SqlRunTool;

pub(super) fn register_batch_tool_for_plan_state(
    reg: &mut ToolRegistry,
    phase: control_flow::Phase,
    plan_state: &PlanState,
    datasets: &Option<std::sync::Arc<dyn crate::providers::DatasetCatalogProvider>>,
) -> Option<&'static str> {
    match (phase, plan_state) {
        (control_flow::Phase::CleanseAuthor, PlanState::CleanseSqlDatasetIds(_)) => {
            reg.register(tools::apply_next_batch::ApplyNextCleanseBatchTool {
                datasets: datasets.clone(),
            });
            Some("apply_next_cleanse_batch")
        }
        (control_flow::Phase::CleanseAuthor, PlanState::CleanseSchemaDatasetIds(_)) => {
            reg.register(
                tools::apply_next_schema_batch::ApplyNextCleanseSchemaBatchTool {
                    datasets: datasets.clone(),
                },
            );
            Some("apply_next_cleanse_schema_batch")
        }
        (control_flow::Phase::ModelAuthor, PlanState::ModelSqlItemNames(_)) => {
            reg.register(tools::apply_next_batch::ApplyNextModelBatchTool);
            Some("apply_next_model_batch")
        }
        (control_flow::Phase::ModelAuthor, PlanState::ModelSchemaItemNames(_)) => {
            reg.register(
                tools::apply_next_schema_batch::ApplyNextModelSchemaBatchTool {
                    datasets: datasets.clone(),
                },
            );
            Some("apply_next_model_schema_batch")
        }
        _ => None,
    }
}

pub(super) enum FileAccessPolicy {
    ReadOnly { error_message: &'static str },
}

pub(super) struct PolicyFilesTool {
    pub(super) inner: tools::files_tool::FilesTool,
    pub(super) policy: FileAccessPolicy,
}

#[async_trait::async_trait]
impl react_core::tools::Tool for PolicyFilesTool {
    fn name(&self) -> &'static str {
        "file"
    }

    async fn call(
        &self,
        args: serde_json::Value,
        ctx: &react_core::agent::AgentCtx,
    ) -> Result<serde_json::Value, String> {
        match self.policy {
            FileAccessPolicy::ReadOnly { error_message } => {
                if !crate::tool_ops::is_file_read_op(&args) {
                    return Err(error_message.to_string());
                }
            }
        }
        self.inner.call(args, ctx).await
    }
}

/// Thread-derived guard: blocks repeated dbt_validate after failure until a mutation occurs,
/// and enforces a data probe after runtime (build/run) failures.
pub(super) struct ThreadDerivedDbtValidateTool {
    pub(super) inner: tools::dbt_validate::DbtValidateTool,
}
#[async_trait::async_trait]
impl react_core::tools::Tool for ThreadDerivedDbtValidateTool {
    fn name(&self) -> &'static str {
        "dbt_validate"
    }
    async fn call(
        &self,
        args: serde_json::Value,
        ctx: &react_core::agent::AgentCtx,
    ) -> Result<serde_json::Value, String> {
        let build = args.get("build").and_then(|v| v.as_bool()).unwrap_or(false);
        let run = args.get("run").and_then(|v| v.as_bool()).unwrap_or(false);
        let runtime_validate = build || run;
        if let (Some(store), Some(tid)) = (ctx.thread_store().as_ref(), ctx.thread_id().as_deref())
        {
            let guard =
                crate::progress_controller::ExecutionState::load(&store.control_store(), tid)
                    .await
                    .map_err(|e| {
                        format!("failed to load execution state for tool policy guard: {e}")
                    })?
                    .map(|st| crate::control_flow::derive_guard_state_from_execution_state(&st))
                    .unwrap_or_default();
            if guard.last_validate_failed && !guard.mutated_since_fail {
                return Err(crate::controller_kernel::guard_block_error(
                    crate::controller_kernel::GuardReason::MutationRequiredAfterValidateFailure,
                ));
            }
            if runtime_validate && guard.probe_required && !guard.probe_satisfied {
                return Err(crate::controller_kernel::guard_block_error(
                    crate::controller_kernel::GuardReason::ProbeRequiredAfterRuntimeFailure,
                ));
            }
        }
        self.inner.call(args, ctx).await
    }
}

pub(super) struct ProbeAwareRunSqlTool {
    pub(super) inner: SqlRunTool,
}
#[async_trait::async_trait]
impl react_core::tools::Tool for ProbeAwareRunSqlTool {
    fn name(&self) -> &'static str {
        "run_sql"
    }
    async fn call(
        &self,
        args: serde_json::Value,
        ctx: &react_core::agent::AgentCtx,
    ) -> Result<serde_json::Value, String> {
        let sql = args
            .get("sql")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if let (Some(store), Some(thread_id)) =
            (ctx.thread_store().as_ref(), ctx.thread_id().as_deref())
        {
            let mut es =
                crate::progress_controller::ExecutionState::load(&store.control_store(), thread_id)
                    .await
                    .map_err(|e| {
                        format!("failed to load execution state for probe-aware run_sql: {e}")
                    })?
                    .unwrap_or_else(crate::progress_controller::ExecutionState::new);
            if matches!(
                es.probe_requirement_status(),
                crate::progress_controller::ProbeRequirementStatus::ExhaustedRequireMutation
            ) {
                return Err("run_sql probe loop exhausted for this validate-failure cycle; apply a mutating file fix before probing again.".to_string());
            }
            let res = self.inner.call(args, ctx).await;
            if es.telemetry.last_validate.as_ref().and_then(|lv| lv.ok) == Some(false)
                && es.hard_mutation_repair_mode()
            {
                match &res {
                    Ok(v) => {
                        let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
                        let sig = crate::progress_controller::ProbeSignature::from_run_sql(&sql, v);
                        let _ = es.note_probe_attempt(&sql, ok, sig);
                    }
                    Err(_) => {
                        let sig = crate::progress_controller::ProbeSignature::from_run_sql(
                            &sql,
                            &serde_json::json!({}),
                        );
                        let _ = es.note_probe_attempt(&sql, false, sig);
                    }
                }
                es.save(&store.control_store(), thread_id).await?;
            }
            return res;
        }
        self.inner.call(args, ctx).await
    }
}
