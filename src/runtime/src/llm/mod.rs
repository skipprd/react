pub use react_core::llm::{ChatMessage, ChatRole, LargeLanguageModel};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum LlmProviderType {
    Local,
    OpenAICompat,
}

#[derive(Clone, Debug)]
pub struct LlmConfig {
    pub provider: LlmProviderType,
    pub chat_model: Option<String>,
    pub embed_model: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub gpu_layers: Option<usize>,
    pub context_length: Option<usize>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        LlmConfig {
            provider: LlmProviderType::Local,
            chat_model: None,
            embed_model: None,
            base_url: None,
            api_key: None,
            gpu_layers: None,
            context_length: Some(4096),
        }
    }
}

/// Factory to build an LLM from configuration.
pub fn create_llm(_cfg: &LlmConfig) -> Arc<dyn LargeLanguageModel> {
    Arc::new(crate::llm::session::RouterModel::new())
}

/// Build LLM config from a resolved `react` config file (with env overrides already applied).
///
/// `LLM_API_KEY` remains env-driven and is intentionally not stored in YAML.
pub fn config_from_resolved(cfg: &crate::config::ReactResolvedConfig) -> LlmConfig {
    use react_core::resolved_config::LlmProvider;
    use crate::runtime_settings as rs;
    let provider = match cfg.llm.provider {
        LlmProvider::Openai | LlmProvider::OpenaiCompat | LlmProvider::Http => LlmProviderType::OpenAICompat,
        LlmProvider::LlamaCpp | LlmProvider::Null => LlmProviderType::Local,
    };
    LlmConfig {
        provider,
        chat_model: cfg.llm.chat_model.clone().or_else(|| rs::llm_chat_model()),
        embed_model: cfg.llm.embed_model.clone().or_else(|| rs::llm_embed_model()),
        base_url: cfg.llm.base_url.clone().or_else(|| rs::llm_base_url()),
        api_key: rs::llm_api_key(),
        gpu_layers: cfg.llm.gpu_layers.or_else(|| rs::llm_gpu_layers()),
        context_length: cfg
            .llm
            .context_length
            .or_else(|| rs::llm_context_length_opt())
            .or(Some(rs::llm_context_length())),
    }
}

/// Build LLM config from the global resolved config (convenience for call sites
/// that don't have access to a `ReactResolvedConfig` reference).
pub fn config_from_global() -> LlmConfig {
    if let Some(cfg) = crate::runtime_settings::resolved_config() {
        config_from_resolved(cfg)
    } else {
        LlmConfig::default()
    }
}

pub mod adapter;
pub mod llama_cpp;
pub mod llama_cpp_adapter;
pub mod openai_chat_adapter;
pub mod openai_compat;
pub mod openai_responses_adapter;
pub mod registry;
pub mod router;
pub mod session;
pub mod thread_ctx;
pub mod types;
