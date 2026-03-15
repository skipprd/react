use react_core::agent::{Agent, AgentCtx, AgentCtxBuilder};
use react_core::llm::{ChatMessage, ChatRole};
use react_core::session::ThreadStore;
use react_core::suite::SuiteCtx;
use react_core::tools::ToolRegistry;
use serde::Deserialize;
use std::sync::Arc;

use crate::control_flow::DeterministicDbtValidateOnce;
use crate::model_dispatch::ModelDispatch;
use crate::repair_session::{
    ApplyResult, FileOp, PlannedFix, RepairErrorContext, RepairIteration, RepairSessionLog,
    ValidateOutcome,
};

const GATHER_MAX_STEPS: usize = 8;
const DEFAULT_MAX_ITERATIONS: usize = 5;

/// Three-stage repair subroutine: Gather → Reason → Apply → Validate.
///
/// Each iteration accumulates into the `RepairSessionLog` so prompts are never identical
/// and the LLM always sees full history of prior attempts.
pub async fn run_repair(
    sctx: &SuiteCtx,
    thread_store: &ThreadStore,
    thread_id: &str,
    dispatch: &ModelDispatch,
    error_context: RepairErrorContext,
    max_iterations: Option<usize>,
) -> Result<RepairSessionLog, String> {
    let max_iters = max_iterations.unwrap_or(DEFAULT_MAX_ITERATIONS);
    let mut session_log = RepairSessionLog::new(error_context);

    for i in 0..max_iters {
        let gathered = run_gather(sctx, thread_id, dispatch, &session_log, i).await?;

        let fix_plan = run_reason(sctx, dispatch, &gathered, &session_log, i).await?;

        let apply_results = apply_fixes(sctx, thread_id, &fix_plan).await;

        session_log.record(
            i,
            gathered.files,
            fix_plan,
            apply_results,
        );

        let has_mutations = session_log
            .last()
            .map(|it| it.has_mutations())
            .unwrap_or(false);

        if has_mutations {
            let actx = build_tool_ctx(sctx, thread_id);
            match DeterministicDbtValidateOnce::run(&actx, true, false, None).await {
                Ok(contract) => {
                    if contract.outcome_v2.ok {
                        session_log.record_validate(ValidateOutcome {
                            passed: true,
                            error_summary: "all tests passed".into(),
                        });
                        index_to_vector_store(sctx, session_log.last().unwrap(), true).await;
                        return Ok(session_log);
                    }
                    let error_summary = serde_json::to_string(&contract.observation)
                        .unwrap_or_else(|_| "validation failed".into());
                    let truncated = if error_summary.len() > 4000 {
                        format!("{}…", &error_summary[..4000])
                    } else {
                        error_summary
                    };
                    session_log.record_validate(ValidateOutcome {
                        passed: false,
                        error_summary: truncated,
                    });
                }
                Err(e) => {
                    session_log.record_validate(ValidateOutcome {
                        passed: false,
                        error_summary: format!("validate execution error: {e}"),
                    });
                }
            }
            index_to_vector_store(sctx, session_log.last().unwrap(), false).await;
        }
    }

    Err(format!(
        "repair_exhausted after {} iterations",
        max_iters
    ))
}

/// Stage 1: Gather — uses the task_model with read tools to investigate the failure.
async fn run_gather(
    sctx: &SuiteCtx,
    thread_id: &str,
    dispatch: &ModelDispatch,
    session_log: &RepairSessionLog,
    iteration: usize,
) -> Result<GatheredContext, String> {
    let registry = build_gather_tools(sctx)?;
    let tools_card = gather_tools_card();

    let actx = AgentCtxBuilder::new(
        sctx.llm().clone(),
        sctx.storage().clone(),
        sctx.scope().clone(),
        sctx.keyspace().clone(),
        Arc::new(react_core::agent::DefaultPolicy),
    )
    .top_k(crate::env_util::DEFAULT_TOP_K)
    .per_step_timeout_secs(10)
    .max_steps(GATHER_MAX_STEPS)
    .thread_id(thread_id)
    .trace_tx(sctx.trace_tx().clone())
    .agent_name("repair_gather")
    .vector(sctx.vector().clone())
    .thread_store(ThreadStore::new(
        sctx.storage().clone(),
        sctx.scope().clone(),
        sctx.keyspace().clone(),
    ))
    .resolved_config(sctx.resolved_config().clone())
    .build();

    let system_prompt = "You are a dbt repair investigator. Read the relevant SQL models, YAML schema files, and query the vector store for similar past errors. Produce a structured diagnosis of the root cause.";

    let history = session_log.format_for_prompt();
    let question = format!(
        "Investigate this dbt failure and diagnose the root cause.\n\n{history}\n\n\
         [Repair iteration {iter} of {max}]\n\n\
         Read the SQL model files and schema YAML that are relevant to the failing tests. \
         Use vect_query to search for similar past repair attempts. \
         After reading, summarise your diagnosis in your final message.",
        iter = iteration + 1,
        max = DEFAULT_MAX_ITERATIONS,
    );

    let options = dispatch.task_call_options("repair_gather");

    let outcome = Agent::run_until_block_non_interactive(
        &registry,
        &actx,
        system_prompt,
        &tools_card,
        &question,
        options,
    )
    .await
    .map_err(|e| format!("gather stage failed: {e}"))?;

    let diagnosis = match outcome {
        react_core::agent::RunOutcomeNonInteractive::Complete { .. } => {
            extract_last_assistant_message(sctx, thread_id).await
        }
        react_core::agent::RunOutcomeNonInteractive::StepBoundary { .. } => {
            extract_last_assistant_message(sctx, thread_id).await
        }
    };

    Ok(GatheredContext {
        files: Vec::new(),
        diagnosis,
    })
}

/// Stage 2: Reason — single LLM call with the reason_model, no tools.
/// Produces a JSON array of planned fixes.
async fn run_reason(
    sctx: &SuiteCtx,
    dispatch: &ModelDispatch,
    gathered: &GatheredContext,
    session_log: &RepairSessionLog,
    iteration: usize,
) -> Result<Vec<PlannedFix>, String> {
    let history = session_log.format_for_prompt();
    let dialect = resolved_config_from_ctx_sctx(sctx)
        .map(|cfg| crate::dialect::active_provider_dialect(cfg))
        .unwrap_or_else(|| "Unknown SQL dialect".into());

    let prompt = format!(
        "You are a senior dbt engineer. Based on the diagnosis and full repair history below, \
         produce a JSON array of fixes.\n\n\
         SQL dialect: {dialect}\n\n\
         ## Diagnosis\n{diagnosis}\n\n\
         ## Full Repair History\n{history}\n\n\
         Respond with ONLY a JSON array. Each element must have:\n\
         - \"path\": relative file path (e.g. \"models/staging/stg_orders.sql\")\n\
         - \"op\": \"patch\" or \"write\"\n\
         - \"content\": for op=patch, a unified diff; for op=write, the complete file content\n\n\
         Rules:\n\
         - If a prior patch attempt failed, use op=write instead.\n\
         - Fix the actual SQL model files when tests fail, not just the schema YAML.\n\
         - Produce at least one fix. An empty array means no progress.",
        diagnosis = gathered.diagnosis,
    );

    let messages = vec![
        ChatMessage {
            role: ChatRole::System,
            content: "You are a dbt repair planner. Output only valid JSON.".to_string(),
        },
        ChatMessage {
            role: ChatRole::User,
            content: prompt,
        },
    ];

    let options = dispatch.reason_call_options("repair_reason", iteration);

    let actx = build_tool_ctx(sctx, "repair_reason");
    let raw = actx
        .llm_chat(&messages, &options)
        .await
        .map_err(|e| format!("reason stage LLM call failed: {e}"))?;

    parse_fix_plan(&raw)
}

/// Stage 3: Apply — mechanically apply each planned fix.
async fn apply_fixes(
    sctx: &SuiteCtx,
    thread_id: &str,
    fixes: &[PlannedFix],
) -> Vec<ApplyResult> {
    let actx = build_tool_ctx(sctx, thread_id);
    let datasets = crate::ctx_ext::sctx_datasets(sctx);
    let mut results = Vec::new();

    for fix in fixes {
        let result = match fix.op {
            FileOp::Patch => {
                let base_state =
                    crate::patch_protocol::read_patch_base_state(&actx, &fix.path).await;
                match crate::patch_protocol::apply_single_file_patch_with_base(
                    &actx,
                    datasets
                        .as_ref()
                        .map(|a| a as &Arc<dyn crate::providers::DatasetCatalogProvider>),
                    &fix.path,
                    &fix.content,
                    &base_state,
                )
                .await
                {
                    Ok(outcome) => {
                        let _ = actx
                            .storage()
                            .put_bytes(
                                &outcome.key,
                                outcome.content.as_bytes(),
                                "text/plain",
                            )
                            .await;
                        ApplyResult {
                            path: fix.path.clone(),
                            op: FileOp::Patch,
                            success: true,
                            error: None,
                        }
                    }
                    Err(e) => ApplyResult {
                        path: fix.path.clone(),
                        op: FileOp::Patch,
                        success: false,
                        error: Some(e),
                    },
                }
            }
            FileOp::Write => {
                match crate::project_fs::write_file(
                    &actx,
                    datasets.as_ref().map(|a| a as &Arc<dyn crate::providers::DatasetCatalogProvider>),
                    &fix.path,
                    &fix.content,
                )
                .await
                {
                    Ok(_) => ApplyResult {
                        path: fix.path.clone(),
                        op: FileOp::Write,
                        success: true,
                        error: None,
                    },
                    Err(e) => ApplyResult {
                        path: fix.path.clone(),
                        op: FileOp::Write,
                        success: false,
                        error: Some(e),
                    },
                }
            }
        };
        results.push(result);
    }

    results
}

fn build_gather_tools(sctx: &SuiteCtx) -> Result<ToolRegistry, String> {
    use crate::tools::{
        files_tool::FilesTool, sql_run::SqlRunTool, sql_sample::SqlSampleTool,
        sql_schema::SqlSchemaTool, sql_stats::SqlStatsTool, vect_query::VectQueryTool,
    };

    let query =
        crate::ctx_ext::sctx_query(sctx).ok_or_else(|| "query provider missing".to_string())?;

    let mut reg = ToolRegistry::new();
    reg.register(FilesTool {
        datasets: crate::ctx_ext::sctx_datasets(sctx),
    });
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
    reg.register(SqlRunTool {
        query: query.clone(),
    });
    reg.register(VectQueryTool);
    Ok(reg)
}

fn gather_tools_card() -> String {
    [
        "Allowed tools (repair gather phase, read-only investigation):",
        "- file(args:{op:\"list\"|\"get\", prefix?:string, path?:string, limit?:int, max_chars?:int})",
        "- sql_schema(args:{table?:string})",
        "- sql_stats(args:{table:string, field:string})",
        "- sql_sample(args:{table:string, field:string, k:int})",
        "- run_sql(args:{sql:string})",
        "- vect_query(args:{scope:\"dataset\"|\"field\"|\"doc\"|\"artifact\"|\"metric\"|\"model\", query_text:string, k:int})",
        "",
        "Not available: file mutations (patch/write/rm/mv), dbt_validate, staging_model, gold_model.",
    ]
    .join("\n")
}

fn build_tool_ctx(sctx: &SuiteCtx, thread_id: &str) -> AgentCtx {
    let thread_store = ThreadStore::new(
        sctx.storage().clone(),
        sctx.scope().clone(),
        sctx.keyspace().clone(),
    );
    let mut actx = AgentCtxBuilder::new(
        sctx.llm().clone(),
        sctx.storage().clone(),
        sctx.scope().clone(),
        sctx.keyspace().clone(),
        Arc::new(react_core::agent::DefaultPolicy),
    )
    .top_k(crate::env_util::DEFAULT_TOP_K)
    .per_step_timeout_secs(10)
    .max_steps(GATHER_MAX_STEPS)
    .thread_id(thread_id)
    .trace_tx(sctx.trace_tx().clone())
    .agent_name("repair")
    .vector(sctx.vector().clone())
    .thread_store(thread_store)
    .resolved_config(sctx.resolved_config().clone())
    .build();
    crate::ctx_ext::copy_capabilities_to_actx(sctx, &mut actx);
    actx
}

async fn extract_last_assistant_message(_sctx: &SuiteCtx, _thread_id: &str) -> String {
    String::new()
}

fn parse_fix_plan(raw: &str) -> Result<Vec<PlannedFix>, String> {
    let trimmed = raw.trim();

    if let Ok(fixes) = serde_json::from_str::<Vec<RawPlannedFix>>(trimmed) {
        return Ok(fixes.into_iter().map(Into::into).collect());
    }

    let repaired = react_core::json_repair::resilient_parse::<serde_json::Value>(trimmed)
        .map_err(|e| format!("failed to parse fix plan JSON: {e}"))?;

    if let Some(arr) = repaired.as_array() {
        let fixes: Vec<RawPlannedFix> = arr
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
        if fixes.is_empty() {
            return Err("fix plan array was empty or contained no valid entries".into());
        }
        return Ok(fixes.into_iter().map(Into::into).collect());
    }

    if let Ok(single) = serde_json::from_value::<RawPlannedFix>(repaired) {
        return Ok(vec![single.into()]);
    }

    Err("could not parse fix plan from LLM response".into())
}

#[derive(Deserialize)]
struct RawPlannedFix {
    path: String,
    op: String,
    content: String,
}

impl From<RawPlannedFix> for PlannedFix {
    fn from(raw: RawPlannedFix) -> Self {
        let op = if raw.op.eq_ignore_ascii_case("write") {
            FileOp::Write
        } else {
            FileOp::Patch
        };
        PlannedFix {
            path: raw.path,
            op,
            content: raw.content,
        }
    }
}

struct GatheredContext {
    files: Vec<(String, String)>,
    diagnosis: String,
}

/// Index a repair iteration to the vector store for future retrieval.
async fn index_to_vector_store(sctx: &SuiteCtx, entry: &RepairIteration, success: bool) {
    let vector = match sctx.vector().as_ref() {
        Some(v) => v,
        None => return,
    };

    let text = format!(
        "Repair attempt ({}): {}\nFiles: {:?}\nDiagnosis: {}",
        if success { "success" } else { "failure" },
        entry.error_brief(),
        entry.files_changed(),
        entry.diagnosis,
    );

    let llm = sctx.llm();
    let embedding = match llm.embed(&[text.clone()]) {
        Ok(mut vecs) if !vecs.is_empty() => vecs.remove(0),
        _ => return,
    };

    let chunk = react_core::provider_traits::vector::VectorChunk {
        id: format!("repair-{}", uuid::Uuid::new_v4()),
        kind: react_core::provider_traits::vector::ChunkKind::Doc,
        entity_id: "repair_attempt".into(),
        field: None,
        text,
        vector: embedding,
        meta: serde_json::json!({
            "outcome": if success { "success" } else { "failure" },
            "files_changed": entry.files_changed(),
        }),
        epoch: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    let _ = vector.upsert(sctx.scope(), &[chunk]).await;
}

fn resolved_config_from_ctx_sctx(
    sctx: &SuiteCtx,
) -> Option<&react_core::resolved_config::ReactResolvedConfig> {
    sctx.resolved_config().as_ref().map(|c| c.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fix_plan_array() {
        let json = r#"[{"path":"models/stg_orders.sql","op":"write","content":"SELECT 1"}]"#;
        let fixes = parse_fix_plan(json).unwrap();
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0].path, "models/stg_orders.sql");
        assert_eq!(fixes[0].op, FileOp::Write);
    }

    #[test]
    fn parse_fix_plan_single_object() {
        let json = r#"{"path":"models/schema.yml","op":"patch","content":"---\n..."}"#;
        let fixes = parse_fix_plan(json).unwrap();
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0].op, FileOp::Patch);
    }

    #[test]
    fn parse_fix_plan_invalid() {
        let result = parse_fix_plan("not json at all");
        assert!(result.is_err());
    }
}
