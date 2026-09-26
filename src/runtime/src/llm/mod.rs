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
    pub reason_model: Option<String>,
    pub task_model: Option<String>,
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
            reason_model: None,
            task_model: None,
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
/// `LLM_API_KEY` or `OPENAI_API_KEY` remains env-driven and is intentionally not stored in YAML.
pub fn config_from_resolved(cfg: &crate::config::ReactResolvedConfig) -> LlmConfig {
    use crate::runtime_settings as rs;
    use react_core::resolved_config::LlmProvider;
    let provider = match cfg.llm.provider {
        LlmProvider::Openai | LlmProvider::OpenaiCompat | LlmProvider::Http => {
            LlmProviderType::OpenAICompat
        }
        LlmProvider::LlamaCpp | LlmProvider::Null => LlmProviderType::Local,
    };
    LlmConfig {
        provider,
        reason_model: cfg
            .llm
            .reason_model
            .clone()
            .or_else(|| rs::llm_reason_model()),
        task_model: cfg.llm.task_model.clone().or_else(|| rs::llm_task_model()),
        embed_model: cfg
            .llm
            .embed_model
            .clone()
            .or_else(|| rs::llm_embed_model()),
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

use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmUsageKind {
    Chat,
    Embed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub model: String,
    pub kind: LlmUsageKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_usage: Option<serde_json::Value>,
}

type LlmUsageHandler = Box<dyn Fn(LlmUsage) + Send + Sync>;
static LLM_USAGE_HANDLER: OnceCell<LlmUsageHandler> = OnceCell::new();

pub use crate::bypass::{BYPASS_AUTH, BYPASS_BILLING};

pub fn set_llm_usage_handler(f: LlmUsageHandler) {
    let _ = LLM_USAGE_HANDLER.set(f);
}

fn emit_llm_usage(bypass_billing: bool, handler: Option<&LlmUsageHandler>, usage: LlmUsage) {
    if bypass_billing {
        return;
    }
    if let Some(handler) = handler {
        handler(usage);
    }
}

pub(crate) fn report_llm_usage(usage: LlmUsage) {
    emit_llm_usage(BYPASS_BILLING, LLM_USAGE_HANDLER.get(), usage);
}

type LlmPreCallGuard = Box<dyn Fn() -> Result<(), String> + Send + Sync>;
static LLM_PRE_CALL_GUARD: OnceCell<LlmPreCallGuard> = OnceCell::new();

pub fn set_llm_pre_call_guard(f: LlmPreCallGuard) {
    let _ = LLM_PRE_CALL_GUARD.set(f);
}

fn run_pre_call_guard(bypass_billing: bool, guard: Option<&LlmPreCallGuard>) -> Result<(), String> {
    if bypass_billing {
        return Ok(());
    }
    match guard {
        Some(guard) => guard(),
        None => Ok(()),
    }
}

pub(crate) fn check_llm_pre_call() -> Result<(), String> {
    run_pre_call_guard(BYPASS_BILLING, LLM_PRE_CALL_GUARD.get())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn sample_usage() -> LlmUsage {
        LlmUsage {
            input_tokens: 1,
            output_tokens: 1,
            model: crate::config::DEFAULT_LLM_MODEL.to_string(),
            kind: LlmUsageKind::Chat,
            prompt_id: None,
            thread_id: None,
            project_id: None,
            provider_usage: None,
        }
    }

    #[test]
    fn billing_is_hard_bypassed() {
        assert!(BYPASS_BILLING);
        assert!(BYPASS_AUTH);
    }

    #[test]
    fn billing_bypass_ignores_failing_pre_call_guard() {
        let guard: LlmPreCallGuard = Box::new(|| Err("balance is $0.00".into()));
        assert_eq!(run_pre_call_guard(true, Some(&guard)), Ok(()));
    }

    #[test]
    fn billing_enforced_when_not_bypassed() {
        let guard: LlmPreCallGuard = Box::new(|| Err("balance is $0.00".into()));
        assert_eq!(
            run_pre_call_guard(false, Some(&guard)),
            Err("balance is $0.00".into())
        );
    }

    #[test]
    fn billing_bypass_does_not_report_usage() {
        static CALLED: AtomicBool = AtomicBool::new(false);
        let handler: LlmUsageHandler = Box::new(|_| {
            CALLED.store(true, Ordering::SeqCst);
        });
        emit_llm_usage(true, Some(&handler), sample_usage());
        assert!(!CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn billing_reports_usage_when_not_bypassed() {
        static CALLED: AtomicBool = AtomicBool::new(false);
        let handler: LlmUsageHandler = Box::new(|_| {
            CALLED.store(true, Ordering::SeqCst);
        });
        emit_llm_usage(false, Some(&handler), sample_usage());
        assert!(CALLED.load(Ordering::SeqCst));
    }
}
