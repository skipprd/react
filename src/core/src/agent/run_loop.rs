use std::sync::Arc;

use tracing::info;

use crate::llm::{ChatMessage, ChatRole, LlmCallOptions, LlmExpectedFormat};
use crate::schema_registry::SchemaId;
use crate::session::{ThreadStep, ToolObservation, ToolStepStatus};
use crate::tools::ToolRegistry;

use super::{
    Agent, AgentCtx, NonInteractivePolicyAdapter, ParsedStep, RunOutcome,
    RunOutcomeNonInteractive, StepBoundaryReason,
};

fn is_retriable_parse_error(e: &str) -> bool {
    e.starts_with("invalid JSON from model:") || e.contains("validation error:")
}

fn build_retry_transcript(transcript: &[String], tail: Option<usize>) -> Vec<String> {
    let mut keep: Vec<String> = Vec::new();
    if let Some(l) = transcript.iter().find(|l| l.starts_with("System:")) {
        keep.push(l.clone());
    }
    if let Some(l) = transcript.iter().find(|l| l.starts_with("Tools:")) {
        keep.push(l.clone());
    }
    if let Some(tail_n) = tail {
        if let Some(l) = transcript.iter().find(|l| l.starts_with("User:")) {
            keep.push(l.clone());
        }
        let n = tail_n.min(transcript.len());
        keep.extend(
            transcript
                .iter()
                .skip(transcript.len().saturating_sub(n))
                .cloned(),
        );
    }
    keep
}

impl Agent {
    pub(crate) async fn llm_chat_once(
        ctx: &AgentCtx,
        prompt: String,
        llm_options: LlmCallOptions,
    ) -> Result<String, String> {
        let messages = vec![ChatMessage {
            role: ChatRole::User,
            content: prompt,
        }];
        ctx.llm_chat(&messages, &llm_options).await
    }

    pub async fn run_until_block_non_interactive(
        tools: &ToolRegistry,
        ctx: &AgentCtx,
        system_prompt: &str,
        tools_card: &str,
        question: &str,
        llm_options: LlmCallOptions,
    ) -> Result<RunOutcomeNonInteractive, String> {
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
    ) -> Result<RunOutcome, String> {
        // Agent steps are a strict JSON Schema contract: enforce provider JSON mode and validate.
        let mut llm_options = llm_options;
        llm_options.expected_format = LlmExpectedFormat::JsonSchema(SchemaId::AgentStepV1);

        let tid = ctx.thread_id.clone().unwrap_or_else(Self::gen_uuid);
        let store = ctx.thread_store.as_ref();

        let output_contract_line = format!(
            "System: OUTPUT_CONTRACT schema_id={}. Return exactly one JSON object matching this schema_id.",
            SchemaId::AgentStepV1.name()
        );

        // Transcript is plain-text lines the model sees.
        let mut transcript: Vec<String> = Vec::new();
        Self::transcript_add(
            &mut transcript,
            format!("System: {}", system_prompt),
            &ctx.trace_tx,
        );
        Self::transcript_add(
            &mut transcript,
            format!("Tools: {}", tools_card),
            &ctx.trace_tx,
        );

        // Suite/policy may inject extra context.
        for l in ctx.policy.prelude_lines(ctx, store, &tid) {
            Self::transcript_add(&mut transcript, l, &ctx.trace_tx);
        }

        Self::transcript_add(
            &mut transcript,
            format!("User: {}", question),
            &ctx.trace_tx,
        );

        for step_idx in 0..ctx.max_steps {
            if let Some(tx) = ctx.progress_tx.as_ref() {
                let _ = tx.send(step_idx);
            }
            if let Some(tx) = ctx.pre_step_tx.as_ref() {
                let _ = tx.send(format!("step {}", step_idx + 1));
            }

            // Ask model for next action.
            let prompt = Self::prompt_from_transcript(ctx, &mut transcript, &output_contract_line);
            let mut raw = Self::llm_chat_once(ctx, prompt, llm_options.clone()).await?;
            // If the provider returned a deterministic error payload as "text", do not enter the
            // invalid-JSON repair ladder (it only wastes tokens and repeats the same failure).
            if raw.trim_start().starts_with("LLM_ERROR:") {
                return Err(raw.trim().to_string());
            }
            let step = match Self::parse_agent_step(&raw) {
                Ok(v) => v,
                Err(e) => {
                    // Defense in depth: if the model output is invalid JSON or fails schema validation,
                    // retry with a minimal prompt so we don't amplify prompt bloat.
                    if !is_retriable_parse_error(&e) {
                        return Err(e);
                    }

                    let resp_hash = crate::llm_observability::sha256_hex_str(&raw);
                    let err_label = if e.contains("validation error:") {
                        "schema_validation_failed"
                    } else {
                        "invalid_json_from_model"
                    };
                    Self::transcript_add(
                        &mut transcript,
                        format!(
                            "Observation: {}",
                            serde_json::json!({
                                "ok": false,
                                "error": err_label,
                                "detail": e,
                                "response_hash": resp_hash,
                                "bytes": raw.as_bytes().len(),
                            })
                        ),
                        &ctx.trace_tx,
                    );

                    let mut keep = build_retry_transcript(&transcript, Some(12));
                    keep.push(format!(
                        "User: IMPORTANT: Your previous response did not match schema {}. Error: {}. Return ONLY one JSON object that matches the schema.",
                        SchemaId::AgentStepV1.name(),
                        e
                    ));
                    let retry_prompt = format!("{}\n{}", keep.join("\n"), output_contract_line);
                    raw = Self::llm_chat_once(ctx, retry_prompt, llm_options.clone()).await?;
                    match Self::parse_agent_step(&raw) {
                        Ok(v) => v,
                        Err(e2) => {
                            if !is_retriable_parse_error(&e2) {
                                return Err(e2);
                            }
                            let mut keep2 = build_retry_transcript(&transcript, None);
                            keep2.push(format!(
                                "User: Return ONLY one JSON object matching schema {}. Error: {}.",
                                SchemaId::AgentStepV1.name(),
                                e2
                            ));
                            let retry_prompt2 =
                                format!("{}\n{}", keep2.join("\n"), output_contract_line);
                            raw = Self::llm_chat_once(ctx, retry_prompt2, llm_options.clone())
                                .await?;
                            Self::parse_agent_step(&raw)?
                        }
                    }
                }
            };
            let (action_name, args) = match step {
                ParsedStep::Complete { complete_env: env } => {
                    if let Some(outcome) = ctx
                        .policy
                        .handle_complete(tools, ctx, &mut transcript, store, &tid, &env)
                        .await?
                    {
                        return Ok(outcome);
                    }
                    continue;
                }
                ParsedStep::Tool { name, args } => (name, args),
            };
            let action_name_str = action_name.as_str();

            let (raw_obs, obs_env) = Self::execute_tool_call(
                tools, ctx, store, &tid, action_name_str, &args,
            )
            .await;

            // Policy may turn this tool into an interrupt.
            if let Some((kind, prompt)) = ctx
                .policy
                .interrupt_for_action(action_name_str, &args, &raw_obs)
            {
                return Ok(RunOutcome::Interrupt {
                    thread_id: tid,
                    kind,
                    prompt,
                });
            }

            Self::append_observation_to_transcript(
                ctx, &mut transcript, &raw, &raw_obs, &obs_env,
            );
        }

        ctx.policy
            .fallback(tools, ctx, &mut transcript, store, &tid)
            .await
    }

    async fn execute_tool_call(
        tools: &ToolRegistry,
        ctx: &AgentCtx,
        store: Option<&crate::session::ThreadStore>,
        tid: &str,
        action_name: &str,
        args: &serde_json::Value,
    ) -> (serde_json::Value, ToolObservation) {
        info!("agent action: {}", action_name);
        let timeout_secs = ctx
            .policy
            .timeout_for_tool(action_name)
            .unwrap_or(ctx.per_step_timeout_secs)
            .max(1);

        let tool_id = uuid::Uuid::new_v4().to_string();
        let agent = ctx
            .agent_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let clean_name = ctx.policy.clean_tool_name(action_name, args);
        if let Some(store) = store {
            let _ = store
                .append_step(
                    tid,
                    ThreadStep::ToolStart {
                        tool_id: tool_id.clone(),
                        name: action_name.to_string(),
                        clean_name: clean_name.clone(),
                        args: args.clone(),
                        status: ToolStepStatus::Running,
                        payload: None,
                        ctx: ctx.exec_ctx.clone(),
                        ts: chrono::Utc::now().to_rfc3339(),
                        agent: agent.clone(),
                    },
                )
                .await;
        }

        let raw_obs = match tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tools.call(action_name, args.clone(), ctx),
        )
        .await
        {
            Ok(r) => r.unwrap_or_else(|e| serde_json::json!({"ok": false, "errors": [e]})),
            Err(_) => serde_json::json!({"ok": false, "errors": ["tool timeout"]}),
        };
        let obs_env = ToolObservation::normalize(raw_obs.clone());

        if let Some(store) = store {
            let status = if obs_env.ok {
                ToolStepStatus::Ok
            } else {
                ToolStepStatus::Failed
            };
            let payload = obs_env
                .extra
                .get("payload")
                .cloned()
                .or_else(|| obs_env.extra.get("ui_payload").cloned());
            let _ = store
                .append_step(
                    tid,
                    ThreadStep::ToolEnd {
                        tool_id: tool_id.clone(),
                        name: action_name.to_string(),
                        clean_name: clean_name.clone(),
                        args: args.clone(),
                        status,
                        payload,
                        ctx: ctx.exec_ctx.clone(),
                        observation: obs_env.clone(),
                        ts: chrono::Utc::now().to_rfc3339(),
                        agent: agent.clone(),
                    },
                )
                .await;
        }

        (raw_obs, obs_env)
    }

    fn append_observation_to_transcript(
        ctx: &AgentCtx,
        transcript: &mut Vec<String>,
        raw_model_output: &str,
        raw_obs: &serde_json::Value,
        obs_env: &ToolObservation,
    ) {
        Self::transcript_add(
            transcript,
            format!("Assistant: {}", raw_model_output),
            &ctx.trace_tx,
        );

        if obs_env.ok {
            Self::transcript_add(
                transcript,
                format!("Observation: {}", raw_obs),
                &ctx.trace_tx,
            );
        } else {
            let max_prompt_chars = crate::error_context::estimate_max_prompt_chars(ctx);
            let used_chars: usize = transcript.iter().map(|l| l.chars().count() + 1).sum();
            let remaining = max_prompt_chars.saturating_sub(used_chars).max(256);
            let rendered =
                crate::error_context::render_failure_context(obs_env, remaining);
            Self::transcript_add(
                transcript,
                format!(
                    "Observation: {}",
                    serde_json::json!({ "ok": false, "error_context": rendered })
                ),
                &ctx.trace_tx,
            );
        }
    }
}
