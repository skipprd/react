use once_cell::sync::OnceCell;
use react_core::resolved_config::{LlmProvider, ReactResolvedConfig};
use std::sync::{Arc, OnceLock};

use crate::runtime_context::RuntimeContext;

// Known limitation: process-global singleton. The long-term plan is to thread
// `RuntimeContext` through all call-sites and remove this static.  For now,
// `build_runtime_context()` below bridges the two worlds.
static RESOLVED_CONFIG: OnceCell<ReactResolvedConfig> = OnceCell::new();

pub fn bind_resolved_config(cfg: &ReactResolvedConfig) {
    let _ = RESOLVED_CONFIG.set(cfg.clone());
}

pub fn resolved_config() -> Option<&'static ReactResolvedConfig> {
    RESOLVED_CONFIG.get()
}

/// Build a [`RuntimeContext`] from the current global config.
pub fn build_runtime_context() -> Option<RuntimeContext> {
    resolved_config().map(|cfg| RuntimeContext {
        config: Arc::new(cfg.clone()),
    })
}

pub fn getenv_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|v| {
        let t = v.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

pub fn getenv_usize(key: &str) -> Option<usize> {
    getenv_nonempty(key).and_then(|v| v.parse::<usize>().ok())
}

pub fn getenv_u64(key: &str) -> Option<u64> {
    getenv_nonempty(key).and_then(|v| v.parse::<u64>().ok())
}

pub fn getenv(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

pub fn llm_provider() -> Option<LlmProvider> {
    resolved_config()
        .map(|cfg| cfg.llm.provider)
        .or_else(|| env_opt("LLM_PROVIDER").and_then(|s| s.parse::<LlmProvider>().ok()))
}

pub fn llm_provider_string() -> String {
    if let Some(provider) = llm_provider() {
        return provider.to_string();
    }
    getenv("LLM_PROVIDER", "OPENAI_COMPAT")
}

pub fn llm_base_url() -> Option<String> {
    resolved_config()
        .and_then(|cfg| cfg.llm.base_url.clone())
        .or_else(|| env_opt("LLM_BASE_URL"))
        .or_else(|| Some(crate::config::DEFAULT_OPENAI_COMPAT_BASE_URL.to_string()))
}

pub fn llm_api_key() -> Option<String> {
    env_opt("LLM_API_KEY").or_else(|| env_opt("OPENAI_API_KEY"))
}

pub fn llm_reason_model() -> Option<String> {
    resolved_config()
        .and_then(|cfg| cfg.llm.reason_model.clone())
        .or_else(|| env_opt("LLM_REASON_MODEL"))
        .or_else(|| Some(crate::config::DEFAULT_LLM_MODEL.to_string()))
}

pub fn llm_task_model() -> Option<String> {
    resolved_config()
        .and_then(|cfg| cfg.llm.task_model.clone())
        .or_else(|| env_opt("LLM_TASK_MODEL"))
        .or_else(|| Some(crate::config::DEFAULT_LLM_MODEL.to_string()))
}

pub fn llm_embed_model() -> Option<String> {
    resolved_config()
        .and_then(|cfg| cfg.llm.embed_model.clone())
        .or_else(|| env_opt("LLM_EMBED_MODEL"))
}

pub fn llm_gpu_layers() -> Option<usize> {
    resolved_config()
        .and_then(|cfg| cfg.llm.gpu_layers)
        .or_else(|| env_opt("LLM_GPU_LAYERS").and_then(|v| v.trim().parse().ok()))
}

pub fn llm_context_length_opt() -> Option<usize> {
    resolved_config()
        .and_then(|cfg| cfg.llm.context_length)
        .or_else(|| env_opt("LLM_CONTEXT_LENGTH").and_then(|v| v.trim().parse().ok()))
}

pub fn llm_context_length() -> usize {
    llm_context_length_opt().unwrap_or(4096)
}

pub fn llm_http_timeout_secs() -> Option<u64> {
    resolved_config().and_then(|cfg| cfg.llm.http_timeout_secs)
}

pub fn llm_max_tokens() -> Option<u32> {
    resolved_config().and_then(|cfg| cfg.llm.max_tokens)
}

pub fn llm_temperature() -> Option<f32> {
    resolved_config().and_then(|cfg| cfg.llm.temperature)
}

/// Default reasoning effort for Luna High (`gpt-5.6-luna` + `high`).
pub fn llm_reasoning_effort() -> react_core::llm::ReasoningEffort {
    react_core::llm::ReasoningEffort::High
}

pub fn llm_top_p() -> Option<f32> {
    resolved_config().and_then(|cfg| cfg.llm.top_p)
}

#[derive(Clone, Debug)]
struct ScopePreference {
    tenant: String,
    workspace: String,
    project_id: String,
}

static SCOPE_PREFERENCE: OnceLock<ScopePreference> = OnceLock::new();

pub fn set_scope_preference(tenant: String, workspace: String, project_id: String) {
    let _ = SCOPE_PREFERENCE.set(ScopePreference {
        tenant,
        workspace,
        project_id,
    });
}

pub fn get_tenant() -> String {
    if let Some(scope) = SCOPE_PREFERENCE.get() {
        let value = scope.tenant.trim();
        if !value.is_empty() {
            return value.to_string();
        }
    }
    getenv("TENANT", "default")
}

pub fn get_workspace_name() -> String {
    if let Some(scope) = SCOPE_PREFERENCE.get() {
        let value = scope.workspace.trim();
        if !value.is_empty() {
            return value.to_string();
        }
    }
    getenv("WORKSPACE", "default")
}

pub fn get_project_id() -> String {
    if let Some(scope) = SCOPE_PREFERENCE.get() {
        let value = scope.project_id.trim();
        if !value.is_empty() {
            return value.to_string();
        }
    }
    getenv("PROJECT_ID", "default")
}

pub fn get_pipeline_name() -> String {
    let project_id = get_project_id();
    if project_id != "default" {
        return project_id;
    }
    getenv("PIPELINE", "default")
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn with_clean_env<F: FnOnce() + std::panic::UnwindSafe>(keys: &[&str], body: F) {
        let _g = ENV_LOCK.lock().unwrap();
        let saved: Vec<(&str, Option<String>)> =
            keys.iter().map(|k| (*k, std::env::var(k).ok())).collect();
        for k in keys {
            std::env::remove_var(k);
        }
        let result = std::panic::catch_unwind(body);
        for (k, v) in saved {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn llm_api_key_falls_back_to_openai_api_key() {
        with_clean_env(&["LLM_API_KEY", "OPENAI_API_KEY"], || {
            std::env::set_var("OPENAI_API_KEY", "sk-openai");
            assert_eq!(llm_api_key().as_deref(), Some("sk-openai"));
        });
    }

    #[test]
    fn llm_api_key_prefers_llm_api_key_over_openai() {
        with_clean_env(&["LLM_API_KEY", "OPENAI_API_KEY"], || {
            std::env::set_var("LLM_API_KEY", "sk-llm");
            std::env::set_var("OPENAI_API_KEY", "sk-openai");
            assert_eq!(llm_api_key().as_deref(), Some("sk-llm"));
        });
    }

    #[test]
    fn llm_reasoning_effort_is_high() {
        assert_eq!(
            llm_reasoning_effort(),
            react_core::llm::ReasoningEffort::High
        );
    }
}
