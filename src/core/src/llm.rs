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

impl LargeLanguageModel for NullModel {
    fn chat(&self, _messages: &[ChatMessage], _options: &LlmCallOptions) -> Result<String, String> {
        Err("LLM provider not configured".to_string())
    }
    fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Err("LLM provider not configured".to_string())
    }
}

pub type DynLlm = Arc<dyn LargeLanguageModel>;

/// Canonical classifier for transient (a.k.a. throttle-shaped) LLM errors.
///
/// This is the **single source of truth** for "is this an upstream rate
/// limit / overload / timeout signal we should react to". It is used by:
///
/// - the retry loop in [`crate::agent::AgentCtx`]'s LLM gateway, and
/// - adaptive concurrency limiters built on top of `AgentCtx` (e.g. the
///   enrichment scatter-gather).
///
/// Keeping the string list in one place avoids the classic bug where two
/// classifiers drift and one path retries while another doesn't.
pub fn is_throttle(msg: &str) -> bool {
    let s = msg.to_ascii_lowercase();
    s.contains("an error occurred while processing your request")
        || s.contains("timeout")
        || s.contains("timed out")
        || s.contains("rate limit")
        || s.contains("too many requests")
        || s.contains("service unavailable")
        || s.contains("internal server error")
        || s.contains("bad gateway")
        || s.contains("gateway timeout")
        || s.contains("http 502")
        || s.contains("http 503")
        || s.contains("http 504")
        || s.contains("http 529")
        || s.contains("overloaded")
        || s.contains("connection reset")
        || s.contains("connection aborted")
}

/// Notification emitted by [`crate::agent::AgentCtx`] when an LLM call hits
/// a throttle-shaped transient. Adaptive limiters subscribe via
/// [`crate::agent::AgentCtx::set_throttle_observer`] to react before the
/// gateway's own backoff completes, so concurrency can shrink immediately.
#[derive(Clone, Debug)]
pub struct LlmThrottleEvent {
    pub prompt_id: &'static str,
    /// Short, lowercased excerpt of the upstream error/message used to make
    /// the throttle classification. Useful for tracing only.
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
mod is_throttle_tests {
    use super::is_throttle;

    #[test]
    fn matches_rate_limit_variants() {
        assert!(is_throttle("rate limit exceeded"));
        assert!(is_throttle("HTTP 429 Too Many Requests"));
        assert!(is_throttle("OpenAI: too many requests"));
    }

    #[test]
    fn matches_overload_and_5xx() {
        assert!(is_throttle("Server overloaded"));
        assert!(is_throttle("HTTP 503 Service Unavailable"));
        assert!(is_throttle("HTTP 502 bad gateway"));
        assert!(is_throttle("HTTP 504 gateway timeout"));
        assert!(is_throttle("HTTP 529 site is overloaded"));
        assert!(is_throttle("internal server error"));
    }

    #[test]
    fn matches_timeout_variants() {
        assert!(is_throttle("connection timed out"));
        assert!(is_throttle("LLM call timeout"));
    }

    #[test]
    fn matches_connection_drops() {
        assert!(is_throttle("connection reset by peer"));
        assert!(is_throttle("Connection aborted unexpectedly"));
    }

    #[test]
    fn rejects_unrelated_errors() {
        assert!(!is_throttle("invalid api key"));
        assert!(!is_throttle("schema validation failed"));
        assert!(!is_throttle("LLM_FATAL_ERROR: billing"));
    }
}
