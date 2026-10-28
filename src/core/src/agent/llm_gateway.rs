use crate::error::CoreError;
use crate::llm::{ChatMessage, ChatRole, LlmCallOptions, LlmError};
use crate::llm_observability;
use crate::retry::retry_classified;
use crate::session::{LlmStepStatus, Observation, ThreadStep};

use super::AgentCtx;

struct ObservabilityContext {
    call_id: Option<u64>,
    phase: String,
    prompt_hash: String,
    parts_built: Option<llm_observability::BuiltParts>,
}

impl AgentCtx {
    /// Single entry point for all LLM chat calls.
    ///
    /// When a thread store and thread_id are available, emits LlmStart/LlmEnd/LlmCall thread events
    /// so the call is visible in UI and thread history. When they are absent (tests, lightweight
    /// contexts), the LLM call proceeds without event emission.
    pub async fn llm_chat(
        &self,
        messages: &[ChatMessage],
        options: &LlmCallOptions,
    ) -> Result<String, CoreError> {
        let thread_id_opt = self.thread_id.clone();
        let store_opt = self.thread_store.clone();
        let agent = self.agent_name_or_default();

        let obs_ctx = self
            .prepare_observability(
                messages,
                options,
                thread_id_opt.as_deref(),
                store_opt.as_ref(),
                &agent,
            )
            .await;

        self.emit_llm_start_step(
            obs_ctx.call_id,
            thread_id_opt.as_deref(),
            store_opt.as_ref(),
            &obs_ctx.phase,
            &agent,
        )
        .await?;

        let mut options = options.clone();
        if options.thread_id.is_none() {
            options.thread_id = thread_id_opt.clone();
        }

        let res = self.invoke_llm(messages, &options).await;

        self.emit_llm_end_step(
            obs_ctx.call_id,
            thread_id_opt.as_deref(),
            store_opt.as_ref(),
            &obs_ctx.phase,
            &agent,
            &res,
        )
        .await?;

        self.record_llm_observation(
            obs_ctx.call_id,
            thread_id_opt.as_deref(),
            store_opt.as_ref(),
            obs_ctx.parts_built,
            &obs_ctx.phase,
            &agent,
            &obs_ctx.prompt_hash,
            &res,
        )
        .await?;

        res
    }

    fn retry_delays() -> &'static [u64] {
        #[cfg(test)]
        {
            &[0, 0]
        }
        #[cfg(not(test))]
        {
            &crate::llm::LLM_TRANSIENT_RETRY_DELAYS_MS
        }
    }

    async fn invoke_llm_once(
        &self,
        messages: &[ChatMessage],
        options: &LlmCallOptions,
    ) -> Result<String, LlmError> {
        if self.cancel.is_cancelled() {
            return Err(LlmError::Cancelled);
        }
        self.budget
            .charge(crate::budget::Charge::LlmCall)
            .map_err(LlmError::Budget)?;
        let chat = self.llm.chat(messages, options);
        let result = if let Some(timeout) = options.timeout_secs {
            tokio::select! {
                _ = self.cancel.cancelled() => Err(LlmError::Cancelled),
                _ = tokio::time::sleep(std::time::Duration::from_secs(timeout)) => {
                    Err(LlmError::Timeout(std::time::Duration::from_secs(timeout)))
                }
                result = chat => result,
            }
        } else {
            tokio::select! {
                _ = self.cancel.cancelled() => Err(LlmError::Cancelled),
                result = chat => result,
            }
        };
        result
    }

    /// Wraps `invoke_llm_once` with retry for [`LlmError::retry`].
    async fn invoke_llm(
        &self,
        messages: &[ChatMessage],
        options: &LlmCallOptions,
    ) -> Result<String, CoreError> {
        retry_classified(
            Self::retry_delays(),
            || async { self.invoke_llm_once(messages, options).await },
            |attempt, err| {
                if let Some(observer) = self.throttle_observer.clone() {
                    observer(crate::llm::LlmThrottleEvent {
                        prompt_id: options.prompt_id,
                        error_summary: err.to_string(),
                        attempt,
                    });
                }
                tracing::warn!(
                    prompt_id = %options.prompt_id,
                    attempt,
                    error = %err,
                    "LLM transient error; retrying after backoff"
                );
            },
        )
        .await
        .map_err(CoreError::from)
    }

    async fn prepare_observability(
        &self,
        messages: &[ChatMessage],
        options: &LlmCallOptions,
        thread_id: Option<&str>,
        store: Option<&crate::session::ThreadStore>,
        agent: &str,
    ) -> ObservabilityContext {
        let obs_enabled = llm_observability::llm_calls_enabled() && thread_id.is_some();
        if !obs_enabled {
            return ObservabilityContext {
                call_id: None,
                phase: "suite_llm".to_string(),
                prompt_hash: String::new(),
                parts_built: None,
            };
        }
        let tid = thread_id.unwrap();
        let call_id = llm_observability::next_call_id(tid);
        let prompt_hash = llm_observability::prompt_hash_for_messages(messages);

        let phase = if let Some(store) = store {
            match store.get(tid).await {
                Ok(log) => {
                    let mut found = "suite_llm".to_string();
                    for step in log.steps.iter().rev() {
                        if let ThreadStep::Phase { phase, .. } = step {
                            let t = phase.trim();
                            if !t.is_empty() {
                                found = t.to_string();
                                break;
                            }
                        }
                    }
                    found
                }
                Err(_) => "suite_llm".to_string(),
            }
        } else {
            "suite_llm".to_string()
        };

        let inputs: Vec<llm_observability::PartInput> = messages
            .iter()
            .enumerate()
            .map(|(i, m)| llm_observability::PartInput {
                name: format!("{}.{}", m.role, i),
                text: m.content.clone(),
            })
            .collect();
        let built = llm_observability::build_parts_for_thread(tid, &inputs);

        tracing::debug!(
            "LLM_CALL thread_id={} call_id={} agent={} phase={} prompt_id={}",
            tid,
            call_id,
            agent,
            phase,
            options.prompt_id
        );

        ObservabilityContext {
            call_id: Some(call_id),
            phase,
            prompt_hash,
            parts_built: Some(built),
        }
    }

    fn resolved_model_name(&self) -> String {
        self.resolved_config
            .as_ref()
            .and_then(|c| c.llm.reason_model.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    async fn emit_llm_start_step(
        &self,
        call_id_opt: Option<u64>,
        thread_id: Option<&str>,
        store_opt: Option<&crate::session::ThreadStore>,
        phase: &str,
        agent: &str,
    ) -> Result<(), CoreError> {
        if let (Some(call_id), Some(tid), Some(store)) = (call_id_opt, thread_id, store_opt) {
            let model = self.resolved_model_name();
            store
                .append_step(
                    tid,
                    ThreadStep::LlmStart {
                        call_id,
                        model: Some(model),
                        phase: phase.to_string(),
                        ctx: self.exec_ctx.clone(),
                        ts: chrono::Utc::now().to_rfc3339(),
                        agent: agent.to_string(),
                    },
                )
                .await?;
        }
        Ok(())
    }

    async fn emit_llm_end_step(
        &self,
        call_id_opt: Option<u64>,
        thread_id: Option<&str>,
        store_opt: Option<&crate::session::ThreadStore>,
        phase: &str,
        agent: &str,
        res: &Result<String, CoreError>,
    ) -> Result<(), CoreError> {
        if let (Some(call_id), Some(tid), Some(store)) = (call_id_opt, thread_id, store_opt) {
            let model = self.resolved_model_name();
            let (ok, response_raw) = match res.as_ref() {
                Ok(txt) => (true, txt.clone()),
                Err(e) => (false, e.to_string()),
            };
            store
                .append_step(
                    tid,
                    ThreadStep::LlmEnd {
                        call_id,
                        model: Some(model),
                        phase: phase.to_string(),
                        status: if ok {
                            LlmStepStatus::Ok
                        } else {
                            LlmStepStatus::Failed
                        },
                        error: if ok { None } else { Some(response_raw) },
                        ctx: self.exec_ctx.clone(),
                        ts: chrono::Utc::now().to_rfc3339(),
                        agent: agent.to_string(),
                    },
                )
                .await?;
        }
        Ok(())
    }

    async fn record_llm_observation(
        &self,
        call_id_opt: Option<u64>,
        thread_id: Option<&str>,
        store_opt: Option<&crate::session::ThreadStore>,
        parts_built: Option<llm_observability::BuiltParts>,
        phase: &str,
        agent: &str,
        prompt_hash: &str,
        res: &Result<String, CoreError>,
    ) -> Result<(), CoreError> {
        if let (Some(call_id), Some(thread_id), Some(store), Some(built)) =
            (call_id_opt, thread_id, store_opt, parts_built)
        {
            let model = self.resolved_model_name();
            let err_string;
            let (ok, response_raw) = match res.as_ref() {
                Ok(txt) => (true, txt.as_str()),
                Err(e) => {
                    err_string = e.to_string();
                    (false, err_string.as_str())
                }
            };

            let response_hash = llm_observability::sha256_hex_str(response_raw);
            let response_text = if llm_observability::llm_response_text_enabled() {
                Some(llm_observability::redact_common_secrets(response_raw))
            } else {
                None
            };

            let ts = chrono::Utc::now().to_rfc3339();
            store
                .append_step(
                    thread_id,
                    ThreadStep::LlmCall {
                        call_id,
                        model,
                        phase: phase.to_string(),
                        prompt_hash: prompt_hash.to_string(),
                        parts: built.parts,
                        part_hashes: built.part_hashes,
                        response_hash,
                        response_text,
                        observation: if ok {
                            Observation::ok()
                        } else {
                            Observation::fail(vec!["llm_call_failed".to_string()])
                        },
                        ts,
                        agent: agent.to_string(),
                    },
                )
                .await?;
        }
        Ok(())
    }

    /// Convenience: `llm_chat` + JSON parse with repair and one retry.
    pub async fn llm_chat_json<T: serde::de::DeserializeOwned>(
        &self,
        messages: &[ChatMessage],
        options: &LlmCallOptions,
    ) -> Result<T, CoreError> {
        let raw = self.llm_chat(messages, options).await?;

        if let Ok(v) = crate::json_repair::resilient_parse::<T>(&raw) {
            return Ok(v);
        }
        let first_err = match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(_) => crate::parse_error::StepParseError::ShapeMismatch(
                "valid JSON of the wrong shape".to_string(),
            ),
            Err(e) => crate::parse_error::StepParseError::NotJson(e.to_string()),
        };

        let mut retry_messages = messages.to_vec();
        retry_messages.push(ChatMessage {
            role: ChatRole::User,
            content: "IMPORTANT: Return ONLY a single valid JSON object. No markdown, no code fences, no prose.".to_string(),
        });
        let raw2 = self.llm_chat(&retry_messages, options).await?;
        crate::json_repair::resilient_parse::<T>(&raw2).map_err(|e2| {
            CoreError::Parse(crate::parse_error::StepParseError::NotJson(format!(
                "{}: {e2} (first_error: {first_err})",
                options.prompt_id
            )))
        })
    }

    /// Embed texts via the underlying LLM provider.
    pub async fn llm_embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        if self.cancel.is_cancelled() {
            return Err(LlmError::Cancelled.into());
        }
        self.budget
            .charge(crate::budget::Charge::LlmCall)
            .map_err(CoreError::from)?;
        tokio::select! {
            _ = self.cancel.cancelled() => Err(LlmError::Cancelled.into()),
            result = self.llm.embed(texts) => result.map_err(CoreError::from),
        }
    }
}
