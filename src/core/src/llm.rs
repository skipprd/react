use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Expected response format for a chat call.
///
/// This is a **hard contract** between callers and providers/adapters.
/// Callers MUST set this explicitly; do not infer it from prompt text.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum LlmExpectedFormat {
    #[default]
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
/// `ExtraHigh` maps to `"high"` for providers that cap at four tiers but
/// signals the intent for maximum deliberation (e.g. larger thinking-token
/// budgets on providers that support it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReasoningEffort {
    None,
    Low,
    Medium,
    High,
    ExtraHigh,
}

/// Per-call overrides for LLM sampling/limits and response contract.
///
/// When optional fields are `None`, implementations should fall back to configured defaults
/// (e.g., env/config values).
#[derive(Clone, Debug, Default)]
pub struct LlmCallOptions {
    /// Stable identifier for the prompt/call site.
    ///
    /// This is REQUIRED so logs/errors can directly name the prompt to tune.
    /// If you see a compile error about missing `prompt_id`, add an explicit id.
    pub prompt_id: &'static str,
    /// Optional thread id (UUID) for observability and provider thread affinity.
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
    /// Callers should assume the runtime default is `High` unless overridden.
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Per-call timeout in seconds. When set, `AgentCtx::llm_chat` wraps the call in
    /// `tokio::time::timeout`. The transport-level HTTP timeout remains as a safety net.
    pub timeout_secs: Option<u64>,
}

/// LLM failures. Success is `Ok(text)`; encoding errors into `Ok` is illegal.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LlmError {
    #[error("fatal: {0}")]
    Fatal(String),
    #[error("transient: {0}")]
    Transient(String),
    #[error("timeout after {0:?}")]
    Timeout(std::time::Duration),
    #[error("cancelled")]
    Cancelled,
    #[error("budget: {0}")]
    Budget(crate::budget::BudgetExhausted),
    #[error("protocol: {0}")]
    Protocol(String),
}

impl LlmError {
    pub fn retry(&self) -> bool {
        matches!(self, Self::Transient(_) | Self::Timeout(_))
    }

    pub fn protocol(msg: impl Into<String>) -> Self {
        Self::Protocol(msg.into())
    }

    pub fn fatal(msg: impl Into<String>) -> Self {
        Self::Fatal(msg.into())
    }

    pub fn transient(msg: impl Into<String>) -> Self {
        Self::Transient(msg.into())
    }
}

/// Provider HTTP bodies MUST NOT be built from an empty chat. One law for every adapter.
pub fn require_nonempty_messages<T>(messages: &[T]) -> Result<(), LlmError> {
    if messages.is_empty() {
        Err(LlmError::protocol("chat request has no messages"))
    } else {
        Ok(())
    }
}

impl crate::retry::Classify for LlmError {
    fn retry(&self) -> bool {
        self.retry()
    }
}

/// Which OpenAI HTTP API a model speaks. Call sites MUST use
/// [`openai_api_for_model`]; grepping model-name prefixes is illegal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenAiApi {
    Responses,
    ChatCompletions,
}

/// Model families that speak the OpenAI Responses API (`gpt-5*` and `o` + digit).
pub fn openai_api_for_model(model: &str) -> OpenAiApi {
    let m = model.trim().to_ascii_lowercase();
    if m.starts_with("gpt-5") {
        return OpenAiApi::Responses;
    }
    let bytes = m.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'o' && bytes[1].is_ascii_digit() {
        return OpenAiApi::Responses;
    }
    OpenAiApi::ChatCompletions
}

/// High-level abstraction for large language models used by the ReAct runtime.
/// Implementations may be local (llama.cpp) or remote (OpenAI-compatible HTTP).
#[async_trait::async_trait]
pub trait LargeLanguageModel: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        options: &LlmCallOptions,
    ) -> Result<String, LlmError>;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError>;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
        match s.trim().to_ascii_lowercase().as_str() {
            "system" => Ok(Self::System),
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            other => Err(format!("unknown ChatRole: '{other}'")),
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

impl Default for NullModel {
    fn default() -> Self {
        Self::new()
    }
}

impl NullModel {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl LargeLanguageModel for NullModel {
    async fn chat(
        &self,
        _messages: &[ChatMessage],
        _options: &LlmCallOptions,
    ) -> Result<String, LlmError> {
        Err(LlmError::Fatal("LLM provider not configured".to_string()))
    }
    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError> {
        Err(LlmError::Fatal("LLM provider not configured".to_string()))
    }
}

/// Host LLM transient backoff (milliseconds). Gateway and out-of-tree
/// callers that retry [`LlmError`] MUST use this array with [`retry_classified`].
pub const LLM_TRANSIENT_RETRY_DELAYS_MS: [u64; 2] = [2000, 4000];

pub type DynLlm = Arc<dyn LargeLanguageModel>;

/// Notification emitted by [`crate::agent::AgentCtx`] when an LLM call hits
/// a throttle-shaped transient. Adaptive limiters subscribe via
/// [`crate::agent::AgentCtx::set_throttle_observer`] to react before the
/// gateway's own backoff completes, so concurrency can shrink immediately.
#[derive(Clone, Debug)]
pub struct LlmThrottleEvent {
    pub prompt_id: &'static str,
    /// Display of the typed [`LlmError`] that classified as retryable.
    pub error_summary: String,
    /// Which retry attempt (1-indexed) triggered the observation, so
    /// observers can distinguish "first failure" from "still throttling".
    pub attempt: usize,
}

/// Type alias for the throttle-observer callback. `Arc` so multiple
/// limiters can be wired up, and `Fn` (not `FnMut`) because emission may
/// happen concurrently from many gateway calls.
pub type LlmThrottleObserver = Arc<dyn Fn(LlmThrottleEvent) + Send + Sync>;

#[cfg(test)]
mod llm_error_tests {
    use super::*;

    #[test]
    fn fatal_and_protocol_are_not_retried() {
        assert!(!LlmError::Fatal("quota".into()).retry());
        assert!(!LlmError::Protocol("empty".into()).retry());
    }

    #[test]
    fn transient_and_timeout_are_retried() {
        assert!(LlmError::Transient("429".into()).retry());
        assert!(LlmError::Timeout(std::time::Duration::from_secs(30)).retry());
    }

    #[test]
    fn openai_api_for_model_is_the_sole_router() {
        assert_eq!(openai_api_for_model("gpt-5.6-luna"), OpenAiApi::Responses);
        assert_eq!(openai_api_for_model("o4-mini"), OpenAiApi::Responses);
        assert_eq!(openai_api_for_model("o3"), OpenAiApi::Responses);
        assert_eq!(openai_api_for_model("gpt-4o"), OpenAiApi::ChatCompletions);
        assert_eq!(
            openai_api_for_model("openai-gpt"),
            OpenAiApi::ChatCompletions
        );
    }

    #[test]
    fn empty_messages_are_protocol() {
        let err = require_nonempty_messages::<u8>(&[]).expect_err("empty");
        assert!(matches!(err, LlmError::Protocol(m) if m.contains("no messages")));
        require_nonempty_messages(&[1u8]).expect("non-empty");
    }

    #[test]
    fn unknown_chat_role_is_rejected() {
        assert_eq!(ChatRole::try_from("USER").unwrap(), ChatRole::User);
        assert!(ChatRole::try_from("tool").is_err());
        assert!(ChatRole::try_from("").is_err());
    }
}
