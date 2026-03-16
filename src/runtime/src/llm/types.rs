use serde::{Deserialize, Serialize};

pub(crate) fn pretty_json(text: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|_| text.to_string()),
        Err(_) => text.to_string(),
    }
}

pub(crate) fn text_from_part(p: &serde_json::Value) -> Option<String> {
    if let Some(s) = p.get("text").and_then(|x| x.as_str()) {
        if !s.trim().is_empty() {
            return Some(s.to_string());
        }
    }
    if let Some(s) = p
        .get("text")
        .and_then(|x| x.get("value"))
        .and_then(|x| x.as_str())
    {
        if !s.trim().is_empty() {
            return Some(s.to_string());
        }
    }
    if let Some(s) = p.get("refusal").and_then(|x| x.as_str()) {
        if !s.trim().is_empty() {
            return Some(s.to_string());
        }
    }
    None
}

/// Classify an OpenAI-style error response into a severity prefix.
/// Permanent errors (quota, auth, billing) get `LLM_FATAL_ERROR:` so callers
/// can fail-fast instead of burning retries on a dead API key.
pub(crate) fn classify_oai_error(error_obj: &serde_json::Value) -> Option<(&'static str, String)> {
    let msg = error_obj.get("message").and_then(|x| x.as_str())?;
    if msg.trim().is_empty() {
        return None;
    }
    let code = error_obj
        .get("code")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let etype = error_obj
        .get("type")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let lower = msg.to_ascii_lowercase();

    let is_permanent = code == "insufficient_quota"
        || code == "billing_hard_limit_reached"
        || code == "account_deactivated"
        || etype == "insufficient_quota"
        || lower.contains("exceeded your current quota")
        || lower.contains("billing")
        || lower.contains("account is not active")
        || lower.contains("invalid api key")
        || lower.contains("incorrect api key")
        || lower.contains("permission denied")
        || lower.contains("organization has been disabled");

    let prefix = if is_permanent {
        "LLM_FATAL_ERROR"
    } else {
        "LLM_ERROR"
    };
    Some((prefix, msg.to_string()))
}

/// Extract the model's text output from an OpenAI-style response.
///
/// When `structured` is true the caller expects a single JSON object (e.g. strict
/// JSON-schema mode). If the `output[]` fallback path finds multiple text chunks we
/// take only the first to avoid silently concatenating independent objects.
pub(crate) fn extract_response_text(v: &serde_json::Value, structured: bool) -> Option<String> {
    if let Some(s) = v.get("output_text").and_then(|x| x.as_str()) {
        if !s.trim().is_empty() {
            return Some(s.to_string());
        }
    }
    if let Some(error_obj) = v.get("error") {
        if let Some((prefix, msg)) = classify_oai_error(error_obj) {
            return Some(format!("{prefix}: {msg}"));
        }
    }
    let mut chunks: Vec<String> = Vec::new();
    if let Some(out) = v.get("output").and_then(|x| x.as_array()) {
        for item in out {
            if let Some(s) = item.get("text").and_then(|x| x.as_str()) {
                if !s.trim().is_empty() {
                    chunks.push(s.to_string());
                }
            }
            if let Some(s) = item.get("refusal").and_then(|x| x.as_str()) {
                if !s.trim().is_empty() {
                    chunks.push(s.to_string());
                }
            }
            if let Some(content) = item.get("content").and_then(|x| x.as_array()) {
                for part in content {
                    if let Some(s) = text_from_part(part) {
                        chunks.push(s);
                    }
                }
            }
        }
    }
    if structured && chunks.len() > 1 {
        tracing::warn!(
            "structured response contained {} output text items; taking only the first",
            chunks.len()
        );
        return chunks.into_iter().next();
    }
    let joined = chunks.join("");
    if joined.trim().is_empty() {
        None
    } else {
        Some(joined)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct OaiChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct OaiChatReq {
    pub model: String,
    pub messages: Vec<OaiChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub(crate) struct OaiChatRespChoiceDelta {
    pub content: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct OaiChatRespChoice {
    pub message: Option<OaiChatMessage>,
    pub delta: Option<OaiChatRespChoiceDelta>,
}

#[derive(Deserialize)]
pub(crate) struct OaiChatResp {
    pub choices: Vec<OaiChatRespChoice>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct OaiEmbReq {
    pub model: String,
    pub input: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct OaiEmbData {
    pub embedding: Vec<f32>,
}

#[derive(Deserialize)]
pub(crate) struct OaiEmbResp {
    pub data: Vec<OaiEmbData>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatResponseFormat {
    Text,
    JsonObject,
    JsonSchema {
        /// Provider-visible schema name (stable).
        name: String,
        /// Full JSON Schema document.
        schema: serde_json::Value,
        /// When supported, enforce strict schema adherence at transport.
        strict: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmExecutionMode {
    Sync,
    BackgroundPreferred,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub response_format: Option<ChatResponseFormat>,
    /// Optional provider hint (e.g. OpenAI reasoning.effort).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// Optional stable identifier for the prompt/call site.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    /// Provider-agnostic execution preference.
    ///
    /// Providers that do not support background execution should ignore this and run synchronously.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_mode: Option<LlmExecutionMode>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ChatResponse {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct EmbedRequest {
    pub model: String,
    pub inputs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct EmbedResponse {
    pub vectors: Vec<Vec<f32>>,
    pub dim: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Capabilities {
    pub supports_responses_api: bool,
    pub supports_stream: bool,
    pub context_window: usize,
    pub embed_input_tokens: usize,
}

#[derive(Clone, Debug)]
pub struct ProviderHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct ProviderHttpResponse {
    pub status: u16,
    pub body_text: String,
    /// True when the request used a structured response format (JsonSchema).
    /// Passed through to `extract_response_text` so it can avoid joining
    /// multiple output items that should be treated as a single object.
    pub structured: bool,
}
