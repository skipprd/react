use super::tool_policies::*;
use super::*;

impl DataEngineerSuite {
    pub(super) fn build_tools(
        agent_mode: AgentMode,
        sctx: &SuiteCtx,
    ) -> Result<ToolRegistry, String> {
        use crate::tools::{
            artifacts::ArtifactsTool, files_tool::FilesTool, sql_run::SqlRunTool,
            sql_sample::SqlSampleTool, sql_schema::SqlSchemaTool, sql_stats::SqlStatsTool,
            vect_query::VectQueryTool,
        };

        let mut registry = ToolRegistry::new();

        let query =
            crate::ctx_ext::sctx_query(sctx).ok_or_else(|| "query provider missing".to_string())?;

        registry.register(SqlSchemaTool {
            query: query.clone(),
            datasets: crate::ctx_ext::sctx_datasets(sctx),
            catalog: crate::ctx_ext::sctx_catalog(sctx),
        });
        registry.register(SqlStatsTool {
            catalog: crate::ctx_ext::sctx_catalog(sctx),
            datasets: crate::ctx_ext::sctx_datasets(sctx),
        });
        registry.register(SqlSampleTool {
            query: query.clone(),
        });
        registry.register(VectQueryTool);

        let allow_user_interrupt_tools = !Self::headless_mode_enabled();
        let caps = Self::agent_capability_profile(agent_mode, allow_user_interrupt_tools);

        if caps.contains(&AgentToolCapability::ReadOnlyFile) {
            registry.register(PolicyFilesTool {
                inner: FilesTool {
                    datasets: crate::ctx_ext::sctx_datasets(sctx),
                },
                policy: FileAccessPolicy::ReadOnly {
                    error_message: "file is read-only for review; use op='get' or op='list' (mutating ops are disabled: patch/rm/mv)",
                },
            });
        } else if caps.contains(&AgentToolCapability::MutableFile) {
            registry.register(FilesTool {
                datasets: crate::ctx_ext::sctx_datasets(sctx),
            });
        }
        if caps.contains(&AgentToolCapability::RunSql) {
            registry.register(SqlRunTool {
                query: query.clone(),
            });
        }
        if caps.contains(&AgentToolCapability::AskUser) {
            registry.register(tools::ask_user::AskUserTool);
        }
        if caps.contains(&AgentToolCapability::AskApproval) {
            registry.register(tools::ask_approval::AskApprovalTool);
        }
        if caps.contains(&AgentToolCapability::SearchDbtExamples) {
            registry.register(tools::dbt_examples::SearchDbtExamplesTool);
        }
        if caps.contains(&AgentToolCapability::StagingModel) {
            registry.register(tools::staging_model::StagingModelTool {
                datasets: crate::ctx_ext::sctx_datasets(sctx),
            });
        }
        if caps.contains(&AgentToolCapability::GoldModel) {
            registry.register(tools::gold_model::GoldModelTool);
        }
        if caps.contains(&AgentToolCapability::DbtValidate) {
            registry.register(ThreadDerivedDbtValidateTool {
                inner: tools::dbt_validate::DbtValidateTool {
                    datasets: crate::ctx_ext::sctx_datasets(sctx),
                    catalog: crate::ctx_ext::sctx_catalog(sctx),
                },
            });
        }
        if caps.contains(&AgentToolCapability::PublishDbt) {
            registry.register(tools::publish_dbt_to_provider::PublishDbtToProviderTool {
                datasets: crate::ctx_ext::sctx_datasets(sctx),
                catalog: crate::ctx_ext::sctx_catalog(sctx),
            });
        }
        if caps.contains(&AgentToolCapability::SqlRegister) {
            registry.register(tools::sql_register::SqlRegisterTool);
        }
        if caps.contains(&AgentToolCapability::CatalogNote) {
            registry.register(tools::catalog_note::CatalogNoteTool);
        }
        if caps.contains(&AgentToolCapability::Artifacts) {
            registry.register(ArtifactsTool);
        }

        Ok(registry)
    }

    pub(super) fn build_tools_card_for_agent_type(agent_mode: AgentMode) -> String {
        let allow_user_interrupt_tools = !Self::headless_mode_enabled();
        let caps = Self::agent_capability_profile(agent_mode, allow_user_interrupt_tools);
        match agent_mode {
            AgentMode::Review => Self::build_tools_card(
                "Allowed tools (review mode, read-only):",
                vec![
                    "- file(args:{op:\"list\"|\"get\", prefix?:string, path?:string, limit?:int, max_chars?:int})".to_string(),
                    "- sql_schema / sql_stats / sql_sample / vect_query (read-only context)".to_string(),
                    "- artifacts".to_string(),
                ],
                Vec::new(),
                Some(
                    "Not available: run_sql, staging_model, gold_model, file patch/rm/mv, dbt_validate, publish_dbt_to_provider."
                        .to_string(),
                ),
            ),
            AgentMode::Ask => {
                let mut lines = vec!["- file(args:{op:\"list\"|\"get\"|\"patch\"|\"rm\"|\"mv\", ...})".to_string(),
                    "- sql_schema / sql_stats / sql_sample / vect_query (discovery context)".to_string()];
                if caps.contains(&AgentToolCapability::RunSql) {
                    lines.push("- run_sql(args:{sql:string})".to_string());
                }
                if caps.contains(&AgentToolCapability::AskApproval) {
                    lines.push("- ask_approval(args:{prompt:string})".to_string());
                }
                if caps.contains(&AgentToolCapability::Artifacts) {
                    lines.push("- artifacts".to_string());
                }
                if caps.contains(&AgentToolCapability::AskUser) {
                    lines.push("- ask_user(args:{prompt:string})".to_string());
                }
                Self::build_tools_card("Allowed tools (ask mode):", lines, Vec::new(), None)
            }
            AgentMode::Agent => {
                let mut lines = vec!["- file(args:{op:\"list\"|\"get\"|\"patch\"|\"rm\"|\"mv\", ...})".to_string(),
                    "- sql_schema / sql_stats / sql_sample / vect_query (discovery context)".to_string()];
                if caps.contains(&AgentToolCapability::RunSql) {
                    lines.push("- run_sql(args:{sql:string})".to_string());
                }
                if caps.contains(&AgentToolCapability::StagingModel)
                    || caps.contains(&AgentToolCapability::GoldModel)
                {
                    lines.push("- staging_model / gold_model".to_string());
                }
                if caps.contains(&AgentToolCapability::DbtValidate)
                    || caps.contains(&AgentToolCapability::PublishDbt)
                {
                    lines.push("- dbt_validate / publish_dbt_to_provider".to_string());
                }
                if caps.contains(&AgentToolCapability::AskApproval) {
                    lines.push("- ask_approval(args:{prompt:string})".to_string());
                }
                if caps.contains(&AgentToolCapability::Artifacts) {
                    lines.push("- artifacts".to_string());
                }
                if caps.contains(&AgentToolCapability::AskUser) {
                    lines.push("- ask_user(args:{prompt:string})".to_string());
                }
                Self::build_tools_card("Allowed tools (model mode):", lines, Vec::new(), None)
            }
        }
    }

    pub(super) fn build_tools_for_phase(
        phase: control_flow::Phase,
        guard: &control_flow::DerivedGuardState,
        _allow_ask_approval: bool,
        sctx: &SuiteCtx,
        plan_state: &PlanState,
        single_target_repair_path: Option<String>,
        suppress_manifest_json_in_plan: bool,
        repair_ladder_step: Option<crate::progress_controller::RepairLadderStep>,
    ) -> Result<(ToolRegistry, String), String> {
        use crate::tools::{
            artifacts::ArtifactsTool, files_tool::FilesTool, json_file::JsonFileTool,
            sql_run::SqlRunTool, sql_sample::SqlSampleTool, sql_schema::SqlSchemaTool,
            sql_stats::SqlStatsTool, vect_query::VectQueryTool,
        };

        let query =
            crate::ctx_ext::sctx_query(sctx).ok_or_else(|| "query provider missing".to_string())?;
        let datasets_opt = crate::ctx_ext::sctx_datasets(sctx);

        let mut reg = ToolRegistry::new();

        // Common read tools (safe in most phases)
        reg.register(SqlSchemaTool {
            query: query.clone(),
            datasets: crate::ctx_ext::sctx_datasets(sctx),
            catalog: crate::ctx_ext::sctx_catalog(sctx),
        });
        reg.register(SqlStatsTool {
            catalog: crate::ctx_ext::sctx_catalog(sctx),
            datasets: crate::ctx_ext::sctx_datasets(sctx),
        });
        reg.register(SqlSampleTool {
            query: query.clone(),
        });
        reg.register(VectQueryTool);
        reg.register(ArtifactsTool);

        let tools_card: String;

        match phase {
            control_flow::Phase::CleansePlan | control_flow::Phase::ModelPlan => {
                // Plan phases: read-only discovery + (optional) probes. No dbt file mutations.
                let suppress_manifest_json =
                    suppress_manifest_json_in_plan && phase == control_flow::Phase::ModelPlan;
                reg.register(SqlRunTool {
                    query: query.clone(),
                });
                reg.register(tools::dbt_examples::SearchDbtExamplesTool);

                reg.register(PolicyFilesTool {
                    inner: FilesTool {
                        datasets: crate::ctx_ext::sctx_datasets(sctx),
                    },
                    policy: FileAccessPolicy::ReadOnly {
                        error_message: "file is read-only in plan phases; use op='get' or op='list' (mutating ops are disabled: patch/rm/mv)",
                    },
                });
                if !suppress_manifest_json {
                    reg.register(JsonFileTool);
                }

                let mut tool_lines: Vec<String> = vec![
                    "- file(args:{op:\"list\", prefix?:string, limit?:int} | {op:\"get\", path:string, max_chars?:int})".to_string(),
                    "- sql_schema / sql_stats / sql_sample / vect_query (discovery context)".to_string(),
                    "- run_sql (targeted probes)".to_string(),
                    "- artifacts".to_string(),
                ];
                if suppress_manifest_json {
                    tool_lines.push("- json_file is temporarily disabled for this model_plan retry due to repeated manifest lookup failures; use deterministic fallback evidence (file + sql_schema + sql_stats/sql_sample).".to_string());
                } else {
                    tool_lines.push("- json_file(args:{op:\"get_item\", path:string, pointer?:string} | {op:\"query\", path:string, pointer?:string, unique_id?:string, name?:string, resource_type?:string, limit?:int})".to_string());
                    tool_lines.push("  - IMPORTANT: use args.op (NOT args.type). For list use args.prefix (NOT path:\".\").".to_string());
                    tool_lines.push("  - For manifest queries use canonical path: target/manifest.json (NOT manifest.json).".to_string());
                }
                tools_card = Self::build_tools_card(
                    "Allowed tools (plan phase, read-only):",
                    tool_lines,
                    Vec::new(),
                    Some(
                        "Not available: staging_model, gold_model, file patch/rm/mv, dbt_validate, publish_dbt_to_provider.".to_string(),
                    ),
                );
            }
            control_flow::Phase::CleanseAuthor | control_flow::Phase::ModelAuthor => {
                // Authoring phases: allow investigation + mutations; validation/publish are suite-driven.
                //
                // Deterministic repair hard-cutover:
                // - classic gate: after validate fail with no mutation yet, require mutation next.
                // - single-target repair mode: ALWAYS require mutation next, even if a prior mutation
                //   already happened in this repair cycle (prevents read/get loops against removed targets).
                let hard_mutation_only = (guard.last_validate_failed && !guard.mutated_since_fail)
                    || single_target_repair_path.is_some();
                let allow_probe_sql = guard.probe_required && !guard.probe_satisfied;
                let plan_state_is_batched = !matches!(plan_state, PlanState::Unconstrained);
                let authoring_policy = crate::authoring_driver::derive_authoring_tool_policy(
                    hard_mutation_only,
                    single_target_repair_path.is_some(),
                    plan_state_is_batched,
                );

                if hard_mutation_only {
                    reg.register(PutOnlyFilesTool {
                        inner: FilesTool {
                            datasets: crate::ctx_ext::sctx_datasets(sctx),
                        },
                        single_target_path: single_target_repair_path.clone(),
                    });

                    if allow_probe_sql {
                        reg.register(ProbeAwareRunSqlTool {
                            inner: SqlRunTool {
                                query: query.clone(),
                            },
                        });
                    }

                    let mut tool_lines: Vec<String> = Vec::new();
                    if !matches!(
                        authoring_policy,
                        crate::authoring_driver::AuthoringToolPolicy::HardMutationSingleTarget
                    ) {
                        let batch_tool_name = register_batch_tool_for_plan_state(
                            &mut reg,
                            phase,
                            plan_state,
                            &datasets_opt,
                        );
                        if let Some(name) = batch_tool_name {
                            tool_lines.push(format!("- {name}(args:{{instructions?:string}})"));
                        }
                    }
                    {
                        use crate::progress_controller::RepairLadderStep;
                        match repair_ladder_step {
                            Some(RepairLadderStep::PatchTarget) => {
                                tool_lines.push("- file(args:{op:\"patch\", ...})".to_string());
                                tool_lines.push("  - op=patch args: {path:string, patch_text:string} (Cursor/Aider hunks-only; patch_text starts with '@@' and MUST NOT include ---/+++ or diff --git)".to_string());
                                tool_lines.push("Deterministic single-target repair mode (patch_target): only file op=patch targeting the current failing model file is allowed.".to_string());
                            }
                            Some(RepairLadderStep::ReplaceContents) => {
                                tool_lines.push("- file(args:{op:\"write\", ...})".to_string());
                                tool_lines.push("  - op=write args: {path:string, content:string} (full file overwrite with complete correct content)".to_string());
                                tool_lines.push("Deterministic single-target repair mode (replace_contents): only file op=write targeting the current failing model file is allowed.".to_string());
                            }
                            Some(RepairLadderStep::FsOp) => {
                                tool_lines.push("- file(args:{op:\"rm\"|\"mv\", ...})".to_string());
                                tool_lines.push("  - op=rm args: {path:string, expected_sha256?:string}".to_string());
                                tool_lines.push("  - op=mv args: {from:string, to:string, expected_sha256?:string}".to_string());
                                tool_lines.push("Deterministic single-target repair mode (fs_op): only file op=rm or op=mv is allowed.".to_string());
                            }
                            Some(RepairLadderStep::Stop) | None => {
                                tool_lines.extend_from_slice(&[
                                    "- file(args:{op:\"patch\"|\"rm\"|\"mv\", ...})".to_string(),
                                    "  - op=patch args: {path:string, patch_text:string} (Cursor/Aider hunks-only; patch_text starts with '@@' and MUST NOT include ---/+++ or diff --git)".to_string(),
                                    "  - op=rm args: {path:string, expected_sha256?:string}".to_string(),
                                    "  - op=mv args: {from:string, to:string, expected_sha256?:string}".to_string(),
                                ]);
                            }
                        }
                    }
                    if allow_probe_sql {
                        tool_lines.push("- run_sql(args:{sql:string}) (targeted probes are currently required by probe gate)".to_string());
                    }
                    tools_card = Self::build_tools_card(
                        "Allowed tools (authoring phase; HARD constraint: mutation required next):",
                        tool_lines,
                        Vec::new(),
                        Some(
                            "Not available: read/explore tools, dbt_validate, publish_dbt_to_provider."
                                .to_string(),
                        ),
                    );
                } else {
                    // Normal authoring: batch tool from PlanState + read/explore tools.
                    let batch_tool_name = register_batch_tool_for_plan_state(
                        &mut reg,
                        phase,
                        plan_state,
                        &datasets_opt,
                    );
                    if matches!(plan_state, PlanState::Unconstrained) {
                        if phase == control_flow::Phase::CleanseAuthor {
                            reg.register(tools::staging_model::StagingModelTool {
                                datasets: crate::ctx_ext::sctx_datasets(sctx),
                            });
                            reg.register(
                                tools::apply_next_schema_batch::ApplyNextCleanseSchemaBatchTool {
                                    datasets: crate::ctx_ext::sctx_datasets(sctx),
                                },
                            );
                        } else {
                            reg.register(tools::gold_model::GoldModelTool);
                            reg.register(
                                tools::apply_next_schema_batch::ApplyNextModelSchemaBatchTool {
                                    datasets: crate::ctx_ext::sctx_datasets(sctx),
                                },
                            );
                        }
                    }
                    reg.register(SqlRunTool {
                        query: query.clone(),
                    });
                    reg.register(tools::dbt_examples::SearchDbtExamplesTool);
                    reg.register(FilesTool {
                        datasets: crate::ctx_ext::sctx_datasets(sctx),
                    });
                    reg.register(JsonFileTool);

                    if let Some(batch_name) = batch_tool_name {
                        let lines = vec![
                            format!("- {batch_name}(args:{{instructions?:string}})"),
                            "- file(args:{op:\"list\"|\"get\", prefix?:string, path?:string, limit?:int, max_chars?:int} | {op:\"patch\", path:string, patch_text:string} | {op:\"rm\", path:string, expected_sha256?:string} | {op:\"mv\", from:string, to:string, expected_sha256?:string})".to_string(),
                            "- json_file(args:{op:\"get_item\", path:string, pointer?:string} | {op:\"query\", path:string, pointer?:string, unique_id?:string, name?:string, resource_type?:string, limit?:int})".to_string(),
                            "- sql_schema / sql_stats / sql_sample / vect_query (discovery context)"
                                .to_string(),
                            "- run_sql (targeted probes)".to_string(),
                            "- artifacts".to_string(),
                        ];
                        tools_card = Self::build_tools_card(
                            "Allowed tools (authoring phase; plan-batched, deterministic):",
                            lines,
                            Vec::new(),
                            Some("Not available in this phase: dbt_validate, publish_dbt_to_provider.".to_string()),
                        );
                    } else {
                        let mut lines = vec![
                            "- sql_schema(args:{table?:string})".to_string(),
                            "- vect_query(args:{scope:\"dataset\"|\"field\"|\"doc\"|\"artifact\"|\"metric\"|\"model\", query_text:string, k:int})".to_string(),
                            "  - IMPORTANT: arg key is query_text (NOT query). scope must be one of the listed strings (NOT \"table\").".to_string(),
                            "- sql_stats(args:{table:string, field:string}) (requires field; no table-only mode)".to_string(),
                            "- sql_sample(args:{table:string, field:string, k:int}) (top values for a FIELD; not a row sampler)".to_string(),
                            "- run_sql(args:{sql:string}) (use this to sample rows: SELECT * FROM <table> LIMIT 20)".to_string(),
                        ];
                        if phase == control_flow::Phase::CleanseAuthor {
                            lines.push("- staging_model(args:{dataset_ids:[string], instructions?:string, sql?:string|staging_model?:string|expression?:string})".to_string());
                            lines.push("  - IMPORTANT: you MUST provide dataset_ids. This tool will NOT default to all datasets.".to_string());
                            lines.push(
                                "- apply_next_cleanse_schema_batch(args:{instructions?:string})"
                                    .to_string(),
                            );
                        } else {
                            lines.push("- gold_model(args:{items:[{name:string, folder?:\"marts\"|\"core\", goal?:string, description?:string, inputs:[string], instructions?:string}]})".to_string());
                            lines.push(format!("  - IMPORTANT: max {} items per call. Gold uses ref() for inputs (stg_* or intra-plan gold models); NO source().", crate::plan_progress::MAX_BATCH_SIZE));
                            lines.push(
                                "- apply_next_model_schema_batch(args:{instructions?:string})"
                                    .to_string(),
                            );
                        }
                        lines.extend_from_slice(&[
                            "- file(args:{op:\"list\"|\"get\", prefix?:string, path?:string, limit?:int, max_chars?:int} | {op:\"patch\", path:string, patch_text:string} | {op:\"rm\", path:string, expected_sha256?:string} | {op:\"mv\", from:string, to:string, expected_sha256?:string})".to_string(),
                            "- json_file(args:{op:\"get_item\", path:string, pointer?:string} | {op:\"query\", path:string, pointer?:string, unique_id?:string, name?:string, resource_type?:string, limit?:int})".to_string(),
                        ]);
                        tools_card = Self::build_tools_card(
                            "Allowed tools (authoring phase):",
                            lines,
                            Vec::new(),
                            Some("Not available in this phase: dbt_validate, publish_dbt_to_provider (suite handles these deterministically).".to_string()),
                        );
                    }
                }
            }
            control_flow::Phase::CleanseReview
            | control_flow::Phase::ModelReview
            | control_flow::Phase::PostPublishReview => {
                // Review phases: keep read-only; do not allow arbitrary SQL execution.
                reg.register(PolicyFilesTool {
                    inner: FilesTool {
                        datasets: crate::ctx_ext::sctx_datasets(sctx),
                    },
                    policy: FileAccessPolicy::ReadOnly {
                        error_message: "file is read-only in review phases; use op='get' or op='list' (mutating ops are disabled: patch/rm/mv)",
                    },
                });
                reg.register(JsonFileTool);

                tools_card = Self::build_tools_card(
                    "Allowed tools (review phase, read-only):",
                    vec![
                        "- file (list/get)".to_string(),
                        "- json_file (get_item/query)".to_string(),
                        "- artifacts".to_string(),
                        "- sql_schema / sql_stats / sql_sample / vect_query (read-only context)"
                            .to_string(),
                    ],
                    Vec::new(),
                    Some("Not available: run_sql, staging_model, approve_and_save_artifact(_batch), dbt_validate, publish_dbt_to_provider.".to_string()),
                );
            }
            _ => {
                // Other phases do not run an LLM action set (suite does deterministic steps).
                tools_card =
                    "Allowed tools: (suite deterministic step; no agent tools)".to_string();
            }
        }

        Ok((reg, tools_card))
    }
}
