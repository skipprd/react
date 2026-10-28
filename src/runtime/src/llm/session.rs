use crate::llm::router::LlmRouter;
use crate::llm::thread_ctx;
use crate::llm::types::ChatResponseFormat;
use crate::llm::types::LlmExecutionMode;
use crate::llm::{ChatMessage, LargeLanguageModel};
use react_core::llm::{LlmCallOptions, LlmError, LlmExpectedFormat, OpenAiApi, ReasoningEffort};

fn openai_schema_name(s: &str) -> String {
    // OpenAI Responses `text.format.name` requires: ^[a-zA-Z0-9_-]+$
    // Our schema IDs include dots; sanitize to a stable provider-safe name.
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "schema".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_schema_name_sanitizes_dots_and_weird_chars() {
        assert_eq!(openai_schema_name("agent.step.v1"), "agent_step_v1");
        assert_eq!(
            openai_schema_name("patch_protocol/single-file@v1"),
            "patch_protocol_single-file_v1"
        );
    }
}

/// Router-backed model to adapt the existing LargeLanguageModel interface
pub struct RouterModel {
    router: LlmRouter,
}

impl RouterModel {
    pub fn new() -> Self {
        Self {
            router: LlmRouter::new(),
        }
    }
}

fn apply_hard_cap_max_output_tokens(requested: Option<u32>) -> Option<u32> {
    // Keep per-call budgets flexible, but enforce one global hard ceiling so a
    // bad prompt cannot explode token usage.
    let cap = crate::runtime_settings::getenv("LLM_MAX_OUTPUT_TOKENS_HARD_CAP", "256000")
        .parse::<u32>()
        .ok()
        .filter(|v| *v > 0)
        .unwrap_or(256000);
    requested.map(|v| v.min(cap))
}

#[async_trait::async_trait]
impl LargeLanguageModel for RouterModel {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        options: &LlmCallOptions,
    ) -> Result<String, LlmError> {
        super::check_llm_pre_call().map_err(LlmError::fatal)?;

        let model = options
            .model
            .clone()
            .or_else(|| crate::runtime_settings::llm_reason_model())
            .unwrap_or_else(|| crate::config::DEFAULT_LLM_MODEL.to_string());

        let default_max_output_tokens = crate::runtime_settings::llm_max_tokens().or_else(|| {
            crate::runtime_settings::getenv("LLM_MAX_TOKENS", "1024")
                .parse()
                .ok()
        });
        let default_temperature = crate::runtime_settings::llm_temperature().or_else(|| {
            crate::runtime_settings::getenv("LLM_TEMPERATURE", "0.2")
                .parse()
                .ok()
        });
        let default_top_p = crate::runtime_settings::llm_top_p().or_else(|| {
            crate::runtime_settings::getenv("LLM_TOP_P", "1.0")
                .parse()
                .ok()
        });

        let max_output_tokens = apply_hard_cap_max_output_tokens(
            options.max_output_tokens.or(default_max_output_tokens),
        );
        let temperature = options.temperature.or(default_temperature);
        let top_p = options.top_p.or(default_top_p);
        let reasoning_effort = match options
            .reasoning_effort
            .unwrap_or_else(crate::runtime_settings::llm_reasoning_effort)
        {
            ReasoningEffort::None => Some("none".to_string()),
            ReasoningEffort::Low => Some("low".to_string()),
            ReasoningEffort::Medium => Some("medium".to_string()),
            ReasoningEffort::High | ReasoningEffort::ExtraHigh => Some("high".to_string()),
        };
        let prompt_id = Some(options.prompt_id.to_string());

        fn should_use_background_mode(model: &str) -> bool {
            let mode =
                crate::runtime_settings::getenv("LLM_BACKGROUND_MODE", "auto").to_ascii_lowercase();
            match mode.as_str() {
                "1" | "true" | "on" | "always" => true,
                "0" | "false" | "off" | "never" => false,
                _ => react_core::llm::openai_api_for_model(model) == OpenAiApi::Responses,
            }
        }
        let execution_mode = if should_use_background_mode(&model) {
            Some(LlmExecutionMode::BackgroundPreferred)
        } else {
            Some(LlmExecutionMode::Sync)
        };

        let req = crate::llm::types::ChatRequest {
            model,
            messages: messages
                .iter()
                .map(|m| crate::llm::types::ChatMessage {
                    role: m.role.to_string(),
                    content: m.content.clone(),
                })
                .collect(),
            max_output_tokens,
            temperature,
            top_p,
            response_format: match &options.expected_format {
                LlmExpectedFormat::Text => None,
                LlmExpectedFormat::JsonObject => Some(ChatResponseFormat::JsonObject),
                LlmExpectedFormat::JsonSchema(schema) => Some(ChatResponseFormat::JsonSchema {
                    name: openai_schema_name(schema.name()),
                    schema: schema.schema().clone(),
                    strict: true,
                }),
            },
            reasoning_effort,
            prompt_id,
            thread_id: options
                .thread_id
                .clone()
                .or_else(|| thread_ctx::current_thread_id()),
            execution_mode,
        };
        let router = self.router.clone();
        tokio::task::spawn_blocking(move || router.chat(&req))
            .await
            .map_err(|e| LlmError::protocol(format!("LLM join: {e}")))?
            .map(|r| r.text)
    }
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let model = crate::runtime_settings::llm_embed_model()
            .unwrap_or_else(|| "text-embedding-3-small".to_string());
        let req = crate::llm::types::EmbedRequest {
            model,
            inputs: texts.to_vec(),
        };
        let router = self.router.clone();
        tokio::task::spawn_blocking(move || router.embed(&req))
            .await
            .map_err(|e| LlmError::protocol(format!("LLM join: {e}")))?
            .map(|r| r.vectors)
    }
}
