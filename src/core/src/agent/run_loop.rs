use std::sync::Arc;

use tracing::info;

use crate::error::CoreError;
use crate::llm::{ChatMessage, LlmCallOptions, LlmError, LlmExpectedFormat};
use crate::schema_registry::SchemaId;
use crate::session::{Observation, RunLoopStopKind, ThreadStep, ToolStepMeta};
use crate::tools::{ToolOutcome, ToolRegistry};

use super::{
    Agent, AgentCtx, AgentStep, CompleteDecision, Entry, NonInteractivePolicyAdapter, RunLoopStop,
    RunOutcome, RunOutcomeNonInteractive, StepBoundaryReason, Transcript,
};

fn is_retriable_parse_error(e: &CoreError) -> bool {
    match e {
        CoreError::Parse(p) => p.retriable(),
        CoreError::Schema(_) => true,
        _ => false,
    }
}

fn trace_entry(tx: &Option<tokio::sync::mpsc::UnboundedSender<String>>, e: &Entry) {
    if let Some(t) = tx.as_ref() {
        let _ = t.send(e.trace());
    }
}

impl Agent {
    async fn record_run_loop_stop(
        ctx: &AgentCtx,
        store: Option<&crate::session::ThreadStore>,
        thread_id: &str,
        stop: &RunLoopStop,
    ) -> Result<(), CoreError> {
        let Some(store) = store else {
            return Ok(());
        };
        let (kind, reason) = match stop {
            RunLoopStop::RejectedComplete { reason } => {
                (RunLoopStopKind::RejectedComplete, reason.clone())
            }
            RunLoopStop::StepLimitExceeded => (
                RunLoopStopKind::StepLimitExceeded,
                "agent reached max_steps without an accepted completion".to_string(),
            ),
            RunLoopStop::PolicyBlocked { reason } => {
                (RunLoopStopKind::PolicyBlocked, reason.clone())
            }
        };
        store
            .append_step(
                thread_id,
                ThreadStep::RunLoopStop {
                    kind,
                    reason,
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: ctx.agent_name_or_default(),
                },
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn llm_chat_messages(
        ctx: &AgentCtx,
        messages: &[ChatMessage],
        llm_options: LlmCallOptions,
    ) -> Result<String, CoreError> {
        ctx.llm_chat(messages, &llm_options).await
    }

    pub async fn run_until_block_non_interactive(
        tools: &ToolRegistry,
        ctx: &AgentCtx,
        system_prompt: &str,
        tools_card: &str,
        question: &str,
        llm_options: LlmCallOptions,
    ) -> Result<RunOutcomeNonInteractive, CoreError> {
        let mut non_interactive_ctx = ctx.clone();
        non_interactive_ctx.policy = Arc::new(NonInteractivePolicyAdapter {
            inner: ctx.policy.clone(),
        });
        match Self::run_until_block(
            tools,
            &non_interactive_ctx,
            system_prompt,
            tools_card,
            question,
            llm_options,
        )
        .await?
        {
            RunOutcome::Complete { thread_id, result } => {
                Ok(RunOutcomeNonInteractive::Complete { thread_id, result })
            }
            RunOutcome::Interrupt {
                thread_id,
                kind: _,
                prompt: _,
            } => Ok(RunOutcomeNonInteractive::StepBoundary {
                thread_id,
                reason: StepBoundaryReason::StepBudgetExhausted,
            }),
        }
    }

    pub async fn run_until_block(
        tools: &ToolRegistry,
        ctx: &AgentCtx,
        system_prompt: &str,
        tools_card: &str,
        question: &str,
        llm_options: LlmCallOptions,
    ) -> Result<RunOutcome, CoreError> {
        let mut llm_options = llm_options;
        llm_options.expected_format = LlmExpectedFormat::JsonSchema(
            crate::schema_registry::OpenAiStrictSchema::for_schema_id(SchemaId::AgentStepV1),
        );

        let tid = ctx.thread_id.clone().unwrap_or_else(Self::gen_uuid);
        let store = ctx.thread_store.as_ref();

        let output_contract = format!(
            "OUTPUT_CONTRACT schema_id={}. Return exactly one JSON object matching this schema_id.",
            SchemaId::AgentStepV1.name()
        );

        let prelude = ctx.policy.prelude_entries(ctx, store, &tid);
        let mut transcript = Transcript::start(
            system_prompt,
            tools_card,
            output_contract,
            prelude,
            question,
        );
        if let Some(tx) = ctx.trace_tx.as_ref() {
            let _ = tx.send(format!("transcript start question={question}"));
        }

        let mut ledger = crate::budget::ActionLedger::default();
        let mut step_idx: usize = 0;
        loop {
            match ctx.budget().charge(crate::budget::Charge::AgentStep) {
                Ok(()) => {}
                Err(crate::budget::BudgetExhausted::AgentSteps) => {
                    Self::record_run_loop_stop(ctx, store, &tid, &RunLoopStop::StepLimitExceeded)
                        .await?;
                    return ctx
                        .policy
                        .fallback(tools, ctx, &mut transcript, store, &tid)
                        .await
                        .map_err(CoreError::generic);
                }
                Err(e) => return Err(e.into()),
            }
            if ctx.cancel().is_cancelled() {
                return Err(LlmError::Cancelled.into());
            }
            if let Some(tx) = ctx.progress_tx.as_ref() {
                let _ = tx.send(step_idx);
            }
            if let Some(tx) = ctx.pre_step_tx.as_ref() {
                let _ = tx.send(format!("step {}", step_idx + 1));
            }
            step_idx += 1;

            let messages = Self::prompt_messages(ctx, &transcript);
            let mut raw = Self::llm_chat_messages(ctx, &messages, llm_options.clone()).await?;
            let step = Self::parse_with_retry(ctx, &mut raw, &mut transcript, &llm_options).await?;
            let (action_name, args) = match step {
                AgentStep::Complete { complete_env: env } => {
                    match ctx
                        .policy
                        .handle_complete(tools, ctx, &mut transcript, store, &tid, &env)
                        .await
                        .map_err(CoreError::generic)?
                    {
                        CompleteDecision::Accept { result } => {
                            return Ok(RunOutcome::Complete {
                                thread_id: tid,
                                result,
                            })
                        }
                        CompleteDecision::Reject { reason } => {
                            transcript.record_error("complete", &reason);
                            Self::record_run_loop_stop(
                                ctx,
                                store,
                                &tid,
                                &RunLoopStop::RejectedComplete {
                                    reason: reason.clone(),
                                },
                            )
                            .await?;
                            continue;
                        }
                    }
                }
                AgentStep::Tool { name, args } => (name, args),
            };
            let action_name_str = action_name.as_str();

            let outcome =
                Self::execute_tool_call(tools, ctx, store, &tid, action_name_str, &args).await;

            if let Some((kind, prompt)) = ctx.policy.interrupt_for_action(
                action_name_str,
                &args,
                &outcome.as_interrupt_value(),
            ) {
                Self::record_run_loop_stop(
                    ctx,
                    store,
                    &tid,
                    &RunLoopStop::PolicyBlocked {
                        reason: format!("{}: {}", kind.as_str(), prompt),
                    },
                )
                .await?;
                return Ok(RunOutcome::Interrupt {
                    thread_id: tid,
                    kind,
                    prompt,
                });
            }

            Self::append_observation_to_transcript(
                ctx,
                &mut transcript,
                &raw,
                action_name_str,
                &outcome,
            );
            if !outcome.is_ok() {
                if let Err(_n) = ledger.record_failure(action_name_str, &args) {
                    Self::record_run_loop_stop(ctx, store, &tid, &RunLoopStop::StepLimitExceeded)
                        .await?;
                    return ctx
                        .policy
                        .fallback(tools, ctx, &mut transcript, store, &tid)
                        .await
                        .map_err(CoreError::generic);
                }
            }
        }
    }

    fn prompt_messages(ctx: &AgentCtx, transcript: &Transcript) -> Vec<ChatMessage> {
        let budget = ctx.prompt_budget();
        let messages = transcript.render(budget);
        if let Some(tx) = ctx.trace_tx.as_ref() {
            let chars: usize = messages.iter().map(|m| m.content.chars().count()).sum();
            let _ = tx.send(format!(
                "prompt_size chars={} messages={} budget_tokens={}",
                chars,
                messages.len(),
                budget.total_tokens.get()
            ));
        }
        messages
    }

    async fn execute_tool_call(
        tools: &ToolRegistry,
        ctx: &AgentCtx,
        store: Option<&crate::session::ThreadStore>,
        tid: &str,
        action_name: &str,
        args: &serde_json::Value,
    ) -> ToolOutcome {
        info!("agent action: {}", action_name);
        if ctx.cancel().is_cancelled() {
            return ToolOutcome::from_result(
                Err("cancelled".into()),
                crate::tools::MAX_OUTPUT_BYTES,
            );
        }
        if let Err(e) = ctx.budget().charge(crate::budget::Charge::ToolCall) {
            return ToolOutcome::from_result(Err(e.to_string()), crate::tools::MAX_OUTPUT_BYTES);
        }
        let timeout_secs = ctx
            .policy
            .timeout_for_tool(action_name)
            .unwrap_or(ctx.per_step_timeout_secs)
            .max(1);

        if let Some(store) = store {
            let meta = ToolStepMeta {
                agent: ctx.agent_name_or_default(),
                phase: "agent_loop".to_string(),
                name: action_name.to_string(),
                clean_name: ctx.policy.clean_tool_name(action_name, args),
                args: args.clone(),
                ctx: ctx.exec_ctx.clone(),
            };
            let tools = tools;
            let action = action_name.to_string();
            let call_args = args.clone();
            let call_ctx = ctx.clone();
            let cancel = ctx.cancel().clone();
            match store
                .run_observed(
                    tid,
                    meta,
                    || async move {
                        match Self::bounded_tool_result(
                            cancel,
                            timeout_secs,
                            tools.call(&action, call_args, &call_ctx),
                        )
                        .await
                        {
                            ToolOutcome::Ok(v) => Ok(v),
                            ToolOutcome::Failed(e) => Err(e.message),
                        }
                    },
                    |raw: &serde_json::Value| Ok(raw.clone()),
                )
                .await
            {
                Ok(v) => ToolOutcome::Ok(v),
                Err(e) => ToolOutcome::Failed(crate::tools::ToolError::new(e)),
            }
        } else {
            Self::bounded_tool_result(
                ctx.cancel().clone(),
                timeout_secs,
                tools.call(action_name, args.clone(), ctx),
            )
            .await
        }
    }

    async fn bounded_tool_result<F>(
        cancel: crate::budget::Cancel,
        timeout_secs: u64,
        fut: F,
    ) -> ToolOutcome
    where
        F: std::future::Future<Output = ToolOutcome>,
    {
        tokio::select! {
            _ = cancel.cancelled() => ToolOutcome::from_result(Err("cancelled".into()), crate::tools::MAX_OUTPUT_BYTES),
            r = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), fut) => match r {
                Ok(outcome) => outcome,
                Err(_) => ToolOutcome::from_result(Err("tool timeout".into()), crate::tools::MAX_OUTPUT_BYTES),
            }
        }
    }

    fn append_observation_to_transcript(
        ctx: &AgentCtx,
        transcript: &mut Transcript,
        raw_model_output: &str,
        tool: &str,
        outcome: &ToolOutcome,
    ) {
        let assistant = Entry::Assistant(raw_model_output.to_string());
        trace_entry(&ctx.trace_tx, &assistant);
        transcript.push(assistant);

        let outcome = match outcome {
            ToolOutcome::Ok(v) => ToolOutcome::Ok(v.clone()),
            ToolOutcome::Failed(err) => {
                transcript.record_error(tool, &err.message);
                let max_chars = ctx.prompt_budget().per_entry_chars();
                let rendered =
                    crate::error_context::render_failure_context(&outcome.observation(), max_chars);
                ToolOutcome::Failed(crate::tools::ToolError::new(rendered))
            }
        };
        let obs = Entry::Observation {
            tool: tool.to_string(),
            outcome,
        };
        trace_entry(&ctx.trace_tx, &obs);
        transcript.push(obs);
    }

    async fn parse_with_retry(
        ctx: &AgentCtx,
        raw: &mut String,
        transcript: &mut Transcript,
        llm_options: &LlmCallOptions,
    ) -> Result<AgentStep, CoreError> {
        match Self::parse_agent_step(raw) {
            Ok(v) => Ok(v),
            Err(e) if !is_retriable_parse_error(&e) => Err(e),
            Err(e) => {
                let err_label = match &e {
                    CoreError::Parse(p) => p.label(),
                    CoreError::Schema(_) => "schema_validation_failed",
                    _ => "invalid_json_from_model",
                };
                let e_str = e.to_string();
                let resp_hash = crate::llm_observability::sha256_hex_str(raw);
                transcript.record_error(err_label, &e_str);
                let rejected = Entry::ParseRejected(format!(
                    "{err_label} {e_str} hash={resp_hash} bytes={}",
                    raw.len()
                ));
                trace_entry(&ctx.trace_tx, &rejected);
                transcript.push(rejected);

                let retry_user = Entry::User(format!(
                    "IMPORTANT: Your previous response did not match schema {}. Error: {}. Return ONLY one JSON object that matches the schema.",
                    SchemaId::AgentStepV1.name(),
                    e_str
                ));
                transcript.push(retry_user);
                let messages = Self::prompt_messages(ctx, transcript);
                *raw = Self::llm_chat_messages(ctx, &messages, llm_options.clone()).await?;
                match Self::parse_agent_step(raw) {
                    Ok(v) => Ok(v),
                    Err(e2) if !is_retriable_parse_error(&e2) => Err(e2),
                    Err(e2) => {
                        let e2_str = e2.to_string();
                        transcript.record_error("parse_retry", &e2_str);
                        transcript.push(Entry::User(format!(
                            "Return ONLY one JSON object matching schema {}. Error: {}.",
                            SchemaId::AgentStepV1.name(),
                            e2_str
                        )));
                        let messages = Self::prompt_messages(ctx, transcript);
                        *raw = Self::llm_chat_messages(ctx, &messages, llm_options.clone()).await?;
                        Self::parse_agent_step(raw)
                    }
                }
            }
        }
    }
}
