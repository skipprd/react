use async_trait::async_trait;
use std::sync::Arc;

use react_core::agent::{Agent, DefaultPolicy, InterruptKind, RunOutcome};
use react_core::session::ThreadStore;
use react_core::suite::{FlowFrame, FlowKind, Suite, SuiteCtx};
use react_core::tools::ToolRegistry;
use react_core::workflow::{PhaseExecutor, PhaseOutcome, WorkflowConfig};

pub struct KbSuite;

pub mod debug;
pub mod prompts;
pub mod tools;

struct KbExecutor<'a> {
    thread_store: ThreadStore,
    sctx: &'a SuiteCtx,
    thread_id: &'a str,
    question: &'a str,
}

#[async_trait]
impl<'a> PhaseExecutor for KbExecutor<'a> {
    async fn execute_turn(&self, _out_frames: &mut Vec<FlowFrame>) -> PhaseOutcome {
        let registry = match KbSuite::build_tools(self.sctx) {
            Ok(r) => r,
            Err(e) => return PhaseOutcome::Failed { reason: e },
        };

        let sys = prompts::system_prompt();
        let tools_card = prompts::tool_card();

        let mut actx = react_core::agent::AgentCtxBuilder::new(
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
        .agent_name("kb")
        .vector(self.sctx.vector().clone())
        .thread_store(self.thread_store.clone())
        .build();
        react_suite_data_engineer::copy_capabilities_to_actx(self.sctx, &mut actx);

        let llm_opts = {
            let max_out: u32 = std::env::var("LLM_KB_MAX_TOKENS")
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(4_000)
                .max(800)
                .min(32_000);
            let effort = match std::env::var("LLM_KB_REASONING_EFFORT")
                .ok()
                .map(|s| s.trim().to_lowercase())
                .as_deref()
            {
                Some("none") => react_core::llm::ReasoningEffort::None,
                Some("low") | None | Some("") => react_core::llm::ReasoningEffort::Low,
                Some("medium") => react_core::llm::ReasoningEffort::Medium,
                Some("high") => react_core::llm::ReasoningEffort::High,
                Some("extra_high") | Some("xhigh") => {
                    react_core::llm::ReasoningEffort::ExtraHigh
                }
                _ => react_core::llm::ReasoningEffort::Low,
            };
            react_core::llm::LlmCallOptions {
                prompt_id: "kb.run",
                thread_id: Some(self.thread_id.to_string()),
                expected_format: react_core::llm::LlmExpectedFormat::JsonObject,
                max_output_tokens: Some(max_out),
                reasoning_effort: Some(effort),
                temperature: None,
                top_p: None,
                timeout_secs: None,
                model: None,
            }
        };

        match Agent::run_until_block(
            &registry,
            &actx,
            sys,
            tools_card,
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

impl KbSuite {
    fn validate_agent_type(agent_type: &str) -> Result<(), String> {
        if agent_type != "kb" {
            return Err(format!(
                "invalid agent_type '{}' for suite 'kb' (expected 'kb')",
                agent_type
            ));
        }
        Ok(())
    }

    fn build_tools(sctx: &SuiteCtx) -> Result<ToolRegistry, String> {
        let mut registry = ToolRegistry::new();
        registry.register(tools::kb_ingest_dir::KbIngestDirTool);
        registry.register(tools::kb_search::KbSearchTool);

        if sctx.vector().is_none() {
            return Err("vector provider missing".to_string());
        }
        Ok(registry)
    }

    async fn run_kb(
        thread_id: &str,
        question: &str,
        sctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        let thread_store = ThreadStore::new(
            sctx.storage().clone(),
            sctx.scope().clone(),
            sctx.keyspace().clone(),
        );

        let executor = KbExecutor {
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
impl Suite for KbSuite {
    fn id(&self) -> &'static str {
        "kb"
    }

    fn label(&self) -> &'static str {
        "KB"
    }

    fn supported_agent_types(&self) -> Vec<String> {
        vec!["kb".to_string()]
    }

    fn default_agent_type(&self) -> &'static str {
        "kb"
    }

    fn phase_order(&self, _agent_type: &str) -> Vec<String> {
        // kb suite does not expose internal phases (single-pass).
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
        let frames = Self::run_kb(thread_id, question, ctx).await?;
        ctx.record_flow_frames(thread_id, agent_type, &frames).await;
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
        let frames = Self::run_kb(thread_id, question, ctx).await?;
        ctx.record_flow_frames(thread_id, agent_type, &frames).await;
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
        let frames = Self::run_kb(thread_id, text, ctx).await?;
        ctx.record_flow_frames(thread_id, agent_type, &frames).await;
        Ok(frames)
    }
}
