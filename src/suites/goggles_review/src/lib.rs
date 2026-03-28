use async_trait::async_trait;
use std::sync::Arc;

use react_core::agent::{Agent, DefaultPolicy, InterruptKind, RunOutcome};
use react_core::session::ThreadStore;
use react_core::suite::{FlowFrame, FlowKind, Suite, SuiteCtx};
use react_core::tools::ToolRegistry;
use react_core::workflow::{PhaseExecutor, PhaseOutcome, WorkflowConfig};

pub mod tools;
pub mod types;

pub use types::*;

pub struct GogglesReviewSuite;

const SYSTEM_PROMPT: &str = r#"You are a document review agent for a bridging loan application platform.

Your job is to:
1. Extract text from uploaded documents using the extract_text tool
2. Assess whether the extracted text satisfies the given requirement using the assess_requirement tool
3. Return the assessment result

Always call extract_text first, then assess_requirement with the extracted text.
If text extraction fails or returns empty, return a needs_human_review verdict."#;

const TOOL_CARD: &str = r#"Available tools:
- extract_text: Download and extract text from a document in S3. Args: {"s3_key": "...", "content_type": "..."}
- assess_requirement: Assess whether extracted text satisfies a requirement. Args: {"extracted_text": "...", "requirement_label": "...", "requirement_description": "...", "question_prompt": "..."}"#;

struct ReviewExecutor<'a> {
    thread_store: ThreadStore,
    sctx: &'a SuiteCtx,
    thread_id: &'a str,
    question: &'a str,
}

#[async_trait]
impl<'a> PhaseExecutor for ReviewExecutor<'a> {
    async fn execute_turn(&self, _out_frames: &mut Vec<FlowFrame>) -> PhaseOutcome {
        let mut registry = ToolRegistry::new();
        registry.register(tools::extract_text::ExtractTextTool);
        registry.register(tools::assess_requirement::AssessRequirementTool);

        let actx = react_core::agent::AgentCtxBuilder::new(
            self.sctx.llm().clone(),
            self.sctx.storage().clone(),
            self.sctx.scope().clone(),
            self.sctx.keyspace().clone(),
            Arc::new(DefaultPolicy),
        )
        .per_step_timeout_secs(120)
        .max_steps(10)
        .thread_id(self.thread_id.to_string())
        .trace_tx(self.sctx.trace_tx().clone())
        .agent_name("goggles_review")
        .thread_store(self.thread_store.clone())
        .build();

        let llm_opts = react_core::llm::LlmCallOptions {
            prompt_id: "goggles_review.run",
            thread_id: Some(self.thread_id.to_string()),
            expected_format: react_core::llm::LlmExpectedFormat::JsonObject,
            max_output_tokens: Some(4_000),
            reasoning_effort: Some(react_core::llm::ReasoningEffort::Medium),
            temperature: Some(0.1),
            top_p: None,
            timeout_secs: Some(120),
            model: None,
        };

        match Agent::run_until_block(
            &registry,
            &actx,
            SYSTEM_PROMPT,
            TOOL_CARD,
            self.question,
            llm_opts,
        )
        .await
        {
            Ok(RunOutcome::Complete {
                thread_id: _,
                result,
            }) => PhaseOutcome::Return(vec![FlowFrame::Complete {
                kind: FlowKind::new(result.kind.clone()),
                payload: result.payload,
                display: result.display,
            }]),
            Ok(RunOutcome::Interrupt {
                thread_id: _,
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

impl GogglesReviewSuite {
    async fn run_review(
        thread_id: &str,
        question: &str,
        sctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        let thread_store = ThreadStore::new(
            sctx.storage().clone(),
            sctx.scope().clone(),
            sctx.keyspace().clone(),
        );

        let executor = ReviewExecutor {
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
impl Suite for GogglesReviewSuite {
    fn id(&self) -> &'static str {
        "goggles_review"
    }

    fn label(&self) -> &'static str {
        "Goggles Document Review"
    }

    fn supported_agent_types(&self) -> Vec<String> {
        vec!["goggles_review".to_string()]
    }

    fn default_agent_type(&self) -> &'static str {
        "goggles_review"
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
        if agent_type != "goggles_review" {
            return Err(format!("invalid agent_type '{}' for goggles_review suite", agent_type));
        }
        let _ = ctx
            .log_writer()
            .ensure_preflight_phase_step(thread_id, agent_type, Some(self.id()), self.initial_phase())
            .await;
        let frames = Self::run_review(thread_id, question, ctx).await?;
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
        self.handle_new(thread_id, question, agent_type, ctx).await
    }

    async fn handle_user(
        &self,
        thread_id: &str,
        text: &str,
        agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        self.handle_new(thread_id, text, agent_type, ctx).await
    }
}
