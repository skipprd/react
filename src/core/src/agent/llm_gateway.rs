use crate::llm::{ChatMessage, ChatRole, LlmCallOptions};
use crate::llm_observability;
use crate::session::{LlmStepStatus, Observation, ThreadStep};

use super::AgentCtx;

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
    ) -> Result<String, String> {
        let model = self.llm.clone();
        let thread_id_opt = self.thread_id.clone();
        let store_opt = self.thread_store.clone();
        let agent = self
            .agent_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        let obs_enabled = llm_observability::llm_calls_enabled() && thread_id_opt.is_some();
        let (call_id_opt, phase, prompt_hash, parts_built) = if obs_enabled {
            let thread_id = thread_id_opt.as_ref().unwrap();
            let call_id = llm_observability::next_call_id(thread_id);
            let prompt_hash = llm_observability::prompt_hash_for_messages(messages);

            let phase =
                if let (Some(store), Some(tid)) = (store_opt.as_ref(), thread_id_opt.as_ref()) {
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
            let built = llm_observability::build_parts_for_thread(thread_id, &inputs);

            tracing::debug!(
                "LLM_CALL thread_id={} call_id={} agent={} phase={} prompt_id={}",
                thread_id,
                call_id,
                agent,
                phase,
                options.prompt_id
            );

            (Some(call_id), phase, prompt_hash, Some(built))
        } else {
            (None, "suite_llm".to_string(), String::new(), None)
        };

        self.emit_llm_start_step(
            call_id_opt, thread_id_opt.as_deref(), store_opt.as_ref(),
            &phase, &agent,
        ).await;

        let mut options = options.clone();
        if options.thread_id.is_none() {
            options.thread_id = thread_id_opt.clone();
        }

        let messages_owned = messages.to_vec();
        let res = if let Some(timeout) = options.timeout_secs {
            match tokio::time::timeout(
                std::time::Duration::from_secs(timeout),
                tokio::task::spawn_blocking(move || model.chat(&messages_owned, &options)),
            )
            .await
            {
                Ok(join_result) => join_result
                    .map_err(|e| format!("LLM execution failed: {}", e))?
                    .map_err(|e| format!("LLM request failed: {}", e)),
                Err(_) => Err(format!("LLM call timed out after {}s", timeout)),
            }
        } else {
            tokio::task::spawn_blocking(move || model.chat(&messages_owned, &options))
                .await
                .map_err(|e| format!("LLM execution failed: {}", e))?
                .map_err(|e| format!("LLM request failed: {}", e))
        };

        self.emit_llm_end_step(
            call_id_opt, thread_id_opt.as_deref(), store_opt.as_ref(),
            &phase, &agent, &res,
        ).await;

        self.record_llm_observation(
            call_id_opt, thread_id_opt.as_deref(), store_opt.as_ref(),
            parts_built, &phase, &agent, &prompt_hash, &res,
        ).await;

        res
    }

    async fn emit_llm_start_step(
        &self,
        call_id_opt: Option<u64>,
        thread_id: Option<&str>,
        store_opt: Option<&crate::session::ThreadStore>,
        phase: &str,
        agent: &str,
    ) {
        if let (Some(call_id), Some(thread_id), Some(store)) =
            (call_id_opt, thread_id, store_opt)
        {
            let _ = store
                .append_step(
                    thread_id,
                    ThreadStep::LlmStart {
                        call_id,
                        model: Some("unknown".to_string()),
                        phase: phase.to_string(),
                        ctx: self.exec_ctx.clone(),
                        ts: chrono::Utc::now().to_rfc3339(),
                        agent: agent.to_string(),
                    },
                )
                .await;
        }
    }

    async fn emit_llm_end_step(
        &self,
        call_id_opt: Option<u64>,
        thread_id: Option<&str>,
        store_opt: Option<&crate::session::ThreadStore>,
        phase: &str,
        agent: &str,
        res: &Result<String, String>,
    ) {
        if let (Some(call_id), Some(thread_id), Some(store)) =
            (call_id_opt, thread_id, store_opt)
        {
            let (ok, response_raw) = match res.as_ref() {
                Ok(txt) => (true, txt.as_str()),
                Err(e) => (false, e.as_str()),
            };
            let _ = store
                .append_step(
                    thread_id,
                    ThreadStep::LlmEnd {
                        call_id,
                        model: Some("unknown".to_string()),
                        phase: phase.to_string(),
                        status: if ok {
                            LlmStepStatus::Ok
                        } else {
                            LlmStepStatus::Failed
                        },
                        error: if ok {
                            None
                        } else {
                            Some(response_raw.to_string())
                        },
                        ctx: self.exec_ctx.clone(),
                        ts: chrono::Utc::now().to_rfc3339(),
                        agent: agent.to_string(),
                    },
                )
                .await;
        }
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
        res: &Result<String, String>,
    ) {
        if let (Some(call_id), Some(thread_id), Some(store), Some(built)) =
            (call_id_opt, thread_id, store_opt, parts_built)
        {
            let (ok, response_raw) = match res.as_ref() {
                Ok(txt) => (true, txt.as_str()),
                Err(e) => (false, e.as_str()),
            };

            let response_hash = llm_observability::sha256_hex_str(response_raw);
            let response_text = if llm_observability::llm_response_text_enabled() {
                Some(llm_observability::redact_common_secrets(response_raw))
            } else {
                None
            };

            let ts = chrono::Utc::now().to_rfc3339();
            let _ = store
                .append_step(
                    thread_id,
                    ThreadStep::LlmCall {
                        call_id,
                        model: "unknown".to_string(),
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
                .await;
        }
    }

    /// Convenience: `llm_chat` + JSON parse with escape-repair and one retry.
    pub async fn llm_chat_json<T: serde::de::DeserializeOwned>(
        &self,
        messages: &[ChatMessage],
        options: &LlmCallOptions,
    ) -> Result<T, String> {
        let raw = self.llm_chat(messages, options).await?;

        if let Ok(v) = serde_json::from_str::<T>(&raw) {
            return Ok(v);
        }

        let repaired = escape_control_chars_in_json_strings(&raw);
        if let Ok(v) = serde_json::from_str::<T>(&repaired) {
            return Ok(v);
        }
        let first_err = serde_json::from_str::<serde_json::Value>(&raw).unwrap_err();

        let mut retry_messages = messages.to_vec();
        retry_messages.push(ChatMessage {
            role: ChatRole::User,
            content: "IMPORTANT: Return ONLY a single valid JSON object. No markdown, no code fences, no prose.".to_string(),
        });
        let raw2 = self.llm_chat(&retry_messages, options).await?;
        serde_json::from_str::<T>(&raw2).map_err(|e2| {
            format!(
                "{}: expected JSON, got parse error: {} (first_error: {})",
                options.prompt_id, e2, first_err
            )
        })
    }

    /// Embed texts via the underlying LLM provider.
    pub fn llm_embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.llm.embed(texts)
    }
}

/// Repair raw control characters inside JSON string literals.
///
/// Some model backends emit "JSON-like" text with literal control characters (e.g. raw newlines)
/// inside string values. This function escapes them so `serde_json` can parse the result.
pub(crate) fn escape_control_chars_in_json_strings(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut in_str = false;
    let mut esc = false;
    for ch in s.chars() {
        if in_str {
            if esc {
                out.push(ch);
                esc = false;
                continue;
            }
            if ch == '\\' {
                out.push(ch);
                esc = true;
                continue;
            }
            match ch {
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{08}' => out.push_str("\\b"),
                '\u{0C}' => out.push_str("\\f"),
                '"' => {
                    out.push(ch);
                    in_str = false;
                }
                c if (c as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                }
                _ => out.push(ch),
            }
            continue;
        }

        if esc {
            out.push(ch);
            esc = false;
            continue;
        }
        match ch {
            '"' => {
                out.push(ch);
                in_str = true;
            }
            '\\' => {
                out.push(ch);
                esc = true;
            }
            _ => out.push(ch),
        }
    }
    out
}
