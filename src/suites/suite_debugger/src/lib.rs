use async_trait::async_trait;
use std::sync::Arc;

use react_core::agent::{Agent, AgentCtxBuilder, DefaultPolicy, InterruptKind, RunOutcome};
use react_core::session::{analysis, ThreadStore};
use react_core::suite::{
    DebugProviderRegistry, FlowFrame, FlowKind, Suite, SuiteCtx,
};
use react_core::tools::ToolRegistry;
use react_core::workflow::{PhaseExecutor, PhaseOutcome, WorkflowConfig};

pub mod capabilities;
pub mod prompts;
pub mod repo_index;
pub mod session;
pub mod tools;

pub struct SuiteDebugger;

struct DebugExecutor<'a> {
    thread_store: ThreadStore,
    sctx: &'a SuiteCtx,
    thread_id: &'a str,
    question: &'a str,
}

#[async_trait]
impl<'a> PhaseExecutor for DebugExecutor<'a> {
    async fn execute_turn(&self, _out_frames: &mut Vec<FlowFrame>) -> PhaseOutcome {
        let debugger_cfg = capabilities::debugger_config(self.sctx);
        let registry = match SuiteDebugger::build_tools(debugger_cfg.enable_admin_repo_query) {
            Ok(r) => r,
            Err(e) => return PhaseOutcome::Failed { reason: e },
        };

        let target_thread_id = extract_target_thread_id(self.question);
        let target_scope = capabilities::target_scope_for_suite(self.sctx);

        let (pre_summary, domain_ctx) = if let Some(tid) = &target_thread_id {
            let target_store = ThreadStore::new(
                self.sctx.storage().clone(),
                target_scope,
                self.sctx.keyspace().clone(),
            );
            let (summary, suite_id) = match target_store.get(tid).await {
                Ok(log) => {
                    let summary = analysis::summarize(&log);
                    let suite_id = extract_target_suite_id(self.question)
                        .or_else(|| infer_target_suite_id(&summary));
                    (Some(summary), suite_id)
                }
                Err(_) => (None, extract_target_suite_id(self.question)),
            };

            let domain = suite_id.as_deref().and_then(|suite_id| {
                self.sctx
                    .capability::<DebugProviderRegistry>()
                    .and_then(|reg| reg.get(suite_id).map(|p| p.domain_context()))
            });

            (summary, domain)
        } else {
            (None, None)
        };

        let default_summary = analysis::ThreadSummary {
            total_steps: 0,
            phases: vec![],
            llm_calls: 0,
            tool_calls: vec![],
            total_duration_ms: None,
            issues: vec![],
            result: None,
        };
        let summary_ref = pre_summary.as_ref().unwrap_or(&default_summary);
        let sys = prompts::system_prompt(summary_ref, domain_ctx, debugger_cfg.strict_audit);
        let tools_card = tools::tool_card(debugger_cfg.enable_admin_repo_query);

        let mut actx = AgentCtxBuilder::new(
            self.sctx.llm().clone(),
            self.sctx.storage().clone(),
            self.sctx.scope().clone(),
            self.sctx.keyspace().clone(),
            Arc::new(DefaultPolicy),
        )
        .top_k(20)
        .per_step_timeout_secs(30)
        .max_steps(30)
        .thread_id(self.thread_id.to_string())
        .trace_tx(self.sctx.trace_tx().clone())
        .agent_name("suite_debugger")
        .vector(self.sctx.vector().clone())
        .thread_store(self.thread_store.clone())
        .build();

        if let Some(caps) = self.sctx.capability::<DebugProviderRegistry>() {
            actx.set_capability(caps);
        }
        if let Some(target_scope) = self.sctx.capability::<capabilities::DebugTargetScope>() {
            actx.set_capability(target_scope);
        }

        let llm_opts = react_core::llm::LlmCallOptions {
            prompt_id: "suite_debugger.run",
            thread_id: Some(self.thread_id.to_string()),
            expected_format: react_core::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: Some(4_000),
            reasoning_effort: Some(debugger_cfg.reasoning_effort),
            temperature: None,
            top_p: None,
            timeout_secs: None,
            model: None,
        };

        match Agent::run_until_block(
            &registry,
            &actx,
            &sys,
            &tools_card,
            self.question,
            llm_opts,
        )
        .await
        {
            Ok(RunOutcome::Complete {
                thread_id: _tid,
                result,
            }) => PhaseOutcome::Return(vec![FlowFrame::Complete {
                kind: FlowKind::new(result.kind.clone()),
                payload: result.payload,
                display: result.display,
            }]),
            Ok(RunOutcome::Interrupt {
                thread_id: _tid,
                kind,
                prompt,
            }) => PhaseOutcome::Return(vec![FlowFrame::Interrupt {
                kind: FlowKind::new(match kind {
                    InterruptKind::AwaitUser => "await_user",
                    InterruptKind::AwaitApproval => "await_approval",
                }),
                prompt,
            }]),
            Err(e) => PhaseOutcome::Failed {
                reason: e.to_string(),
            },
        }
    }

    async fn on_budget_exhausted(&self, _out_frames: &mut Vec<FlowFrame>, _total_steps: usize) {}

    async fn step_count(&self) -> usize {
        self.thread_store
            .get(self.thread_id)
            .await
            .ok()
            .map(|log| log.steps.len())
            .unwrap_or(0)
    }
}

impl SuiteDebugger {
    fn validate_agent_type(agent_type: &str) -> Result<(), String> {
        if agent_type != "suite_debugger" {
            return Err(format!(
                "invalid agent_type '{agent_type}' for suite 'suite_debugger' (expected 'suite_debugger')"
            ));
        }
        Ok(())
    }

    fn build_tools(enable_admin_repo_query: bool) -> Result<ToolRegistry, String> {
        let mut registry = ToolRegistry::new();
        tools::register_all(&mut registry, enable_admin_repo_query);
        Ok(registry)
    }

    async fn run_debug(
        thread_id: &str,
        question: &str,
        sctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        let thread_store = ThreadStore::new(
            sctx.storage().clone(),
            sctx.scope().clone(),
            sctx.keyspace().clone(),
        );

        let executor = DebugExecutor {
            thread_store,
            sctx,
            thread_id,
            question,
        };

        let config = WorkflowConfig {
            max_phase_steps: 1,
            max_consecutive_waiting_idle: 5,
            max_consecutive_waiting_active: 12,
            ..Default::default()
        };

        react_core::workflow::runner::run(&executor, &config).await
    }
}

#[async_trait]
impl Suite for SuiteDebugger {
    fn id(&self) -> &'static str {
        "suite_debugger"
    }

    fn label(&self) -> &'static str {
        "Suite Debugger"
    }

    fn supported_agent_types(&self) -> Vec<String> {
        vec!["suite_debugger".to_string()]
    }

    fn default_agent_type(&self) -> &'static str {
        "suite_debugger"
    }

    fn phase_order(&self, _agent_type: &str) -> Vec<String> {
        Vec::new()
    }

    async fn handle_new(
        &self,
        thread_id: &str,
        question: &str,
        agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        Self::validate_agent_type(agent_type)?;
        let _ = ctx
            .log_writer()
            .ensure_preflight_phase_step(
                thread_id,
                agent_type,
                Some(self.id()),
                self.initial_phase(),
            )
            .await;
        let frames = Self::run_debug(thread_id, question, ctx).await?;
        ctx.record_flow_frames(thread_id, agent_type, &frames)
            .await;
        Ok(frames)
    }

    async fn handle_open(
        &self,
        thread_id: &str,
        question: &str,
        agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        Self::validate_agent_type(agent_type)?;
        let _ = ctx
            .log_writer()
            .ensure_preflight_phase_step(
                thread_id,
                agent_type,
                Some(self.id()),
                self.initial_phase(),
            )
            .await;
        let frames = Self::run_debug(thread_id, question, ctx).await?;
        ctx.record_flow_frames(thread_id, agent_type, &frames)
            .await;
        Ok(frames)
    }

    async fn handle_user(
        &self,
        thread_id: &str,
        text: &str,
        agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        Self::validate_agent_type(agent_type)?;
        let _ = ctx
            .log_writer()
            .ensure_preflight_phase_step(
                thread_id,
                agent_type,
                Some(self.id()),
                self.initial_phase(),
            )
            .await;
        let frames = Self::run_debug(thread_id, text, ctx).await?;
        ctx.record_flow_frames(thread_id, agent_type, &frames)
            .await;
        Ok(frames)
    }
}

fn extract_target_thread_id(question: &str) -> Option<String> {
    for word in question.split_whitespace() {
        let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        if cleaned.len() >= 8
            && cleaned
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == '-')
        {
            return Some(cleaned.to_string());
        }
    }
    None
}

fn extract_target_suite_id(question: &str) -> Option<String> {
    let lower = question.to_lowercase();
    let known = ["data_engineer", "kb", "suite_debugger"];
    for id in &known {
        if lower.contains(id) {
            return Some(id.to_string());
        }
    }
    None
}

fn infer_target_suite_id(summary: &analysis::ThreadSummary) -> Option<String> {
    let phase_names: Vec<&str> = summary.phases.iter().map(|p| p.name.as_str()).collect();
    let tool_names: Vec<&str> = summary.tool_calls.iter().map(|t| t.name.as_str()).collect();

    let looks_like_data_engineer = phase_names.iter().any(|name| {
        matches!(
            *name,
            "preflight"
                | "plan"
                | "author"
                | "review"
                | "validate"
                | "publish"
                | "el_discover"
                | "el_sync"
                | "el_verify"
        )
    }) || tool_names.iter().any(|name| {
        matches!(
            *name,
            "dbt_validate"
                | "vect_query"
                | "publish_dbt_to_provider"
                | "catalog_note"
                | "apply_next_batch"
                | "apply_next_schema_batch"
        )
    });
    if looks_like_data_engineer {
        return Some("data_engineer".to_string());
    }

    let looks_like_kb = phase_names.iter().any(|name| *name == "kb")
        || tool_names
            .iter()
            .any(|name| matches!(*name, "kb_search" | "kb_ingest_dir"));
    if looks_like_kb {
        return Some("kb".to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_data_engineer_suite_from_tools() {
        let summary = analysis::ThreadSummary {
            total_steps: 0,
            phases: vec![],
            llm_calls: 0,
            tool_calls: vec![analysis::ToolCallSummary {
                name: "dbt_validate".to_string(),
                count: 1,
                successes: 1,
                failures: 0,
            }],
            total_duration_ms: None,
            issues: vec![],
            result: None,
        };
        assert_eq!(
            infer_target_suite_id(&summary).as_deref(),
            Some("data_engineer")
        );
    }
}
