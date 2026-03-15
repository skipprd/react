use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Expected response format for a chat call.
///
/// This is a **hard contract** between callers and providers/adapters.
/// Callers MUST set this explicitly; do not infer it from prompt text.
#[derive(Clone, Debug, PartialEq)]
pub enum LlmExpectedFormat {
    Text,
    JsonObject,
    /// A single JSON object matching an OpenAI-compatible strict schema built in core.
    ///
    /// Callers cannot pass raw JSON here; they must use `OpenAiStrictSchema` constructors so
    /// provider-facing schemas are normalized centrally.
    JsonSchema(crate::schema_registry::OpenAiStrictSchema),
}

/// OpenAI-style reasoning effort hint.
///
/// Not all providers support this; unsupported providers should ignore it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReasoningEffort {
    None,
    Low,
    Medium,
    High,
}

impl Default for LlmExpectedFormat {
    fn default() -> Self {
        LlmExpectedFormat::Text
    }
}

/// Per-call overrides for LLM sampling/limits and response contract.
///
/// When optional fields are `None`, implementations should fall back to configured defaults
/// (e.g., env/config values).
#[derive(Clone, Debug)]
pub struct LlmCallOptions {
    /// Stable identifier for the prompt/call site.
    ///
    /// This is REQUIRED so logs/errors can directly name the prompt to tune.
    /// If you see a compile error about missing `prompt_id`, add an explicit id.
    pub prompt_id: &'static str,
    /// Optional thread id (UUID) for observability and provider thread affinity.
    ///
    /// This must be passed explicitly because `spawn_blocking` does not propagate tokio task-locals.
    pub thread_id: Option<String>,
    /// Per-call model override. When `Some`, the runtime uses this model instead of the
    /// default reason_model. Used by `ModelDispatch` to route gather calls to the task_model.
    pub model: Option<String>,
    pub expected_format: LlmExpectedFormat,
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    /// Optional provider hint for additional deliberation.
    ///
    /// Callers should assume the runtime default is `Low` unless overridden.
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Per-call timeout in seconds. When set, `AgentCtx::llm_chat` wraps the call in
    /// `tokio::time::timeout`. The transport-level HTTP timeout remains as a safety net.
    pub timeout_secs: Option<u64>,
}

impl Default for LlmCallOptions {
    fn default() -> Self {
        Self {
            prompt_id: "",
            thread_id: None,
            model: None,
            expected_format: LlmExpectedFormat::default(),
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            reasoning_effort: None,
            timeout_secs: None,
        }
    }
}

/// High-level abstraction for large language models used by the ReAct runtime.
/// Implementations may be local (llama.cpp) or remote (OpenAI-compatible HTTP).
pub trait LargeLanguageModel: Send + Sync {
    fn chat(&self, messages: &[ChatMessage], options: &LlmCallOptions) -> Result<String, String>;
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

impl std::fmt::Display for ChatRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
        }
    }
}

impl TryFrom<&str> for ChatRole {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "system" => Ok(Self::System),
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            other => Err(format!("unknown ChatRole: '{}'", other)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

/// Placeholder model that always errors. Useful for tests that need a default.
pub struct NullModel {}

impl NullModel {
    pub fn new() -> Self {
        Self {}
    }
}

impl LargeLanguageModel for NullModel {
    fn chat(&self, _messages: &[ChatMessage], _options: &LlmCallOptions) -> Result<String, String> {
        Err("LLM provider not configured".to_string())
    }
    fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Err("LLM provider not configured".to_string())
    }
}

pub type DynLlm = Arc<dyn LargeLanguageModel>;
