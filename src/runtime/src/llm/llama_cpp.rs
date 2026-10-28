use super::{ChatMessage, LargeLanguageModel, LlmConfig};
use react_core::llm::LlmError;

mod inner {
    use super::*;
    pub fn chat(
        _cfg: &LlmConfig,
        messages: &[ChatMessage],
        _options: &react_core::llm::LlmCallOptions,
    ) -> String {
        let prompt = messages
            .iter()
            .map(|m| format!("{}: {}\n", m.role, m.content))
            .collect::<String>();
        format!("[local-llm-disabled]\n{}", prompt)
    }
    pub fn embed(_cfg: &LlmConfig, texts: &[String]) -> Vec<Vec<f32>> {
        texts.iter().map(|_| vec![0.0; 768]).collect()
    }
}

/// Local llama.cpp provider. The native `llama-cpp-2` backend is not in this
/// workspace; this type is the sole local-LLM path and returns a disabled stub.
pub struct LlamaCppModel {
    cfg: LlmConfig,
}

impl LlamaCppModel {
    pub fn new(cfg: LlmConfig) -> Self {
        Self { cfg }
    }
}

#[async_trait::async_trait]
impl LargeLanguageModel for LlamaCppModel {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        options: &react_core::llm::LlmCallOptions,
    ) -> Result<String, LlmError> {
        let _ = options;
        Ok(inner::chat(&self.cfg, messages, options))
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError> {
        Ok(inner::embed(&self.cfg, texts))
    }
}
