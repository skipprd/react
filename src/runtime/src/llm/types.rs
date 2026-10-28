use serde::{Deserialize, Serialize};
use std::error::Error as _;
use std::time::Duration;

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

use react_core::llm::{ChatRole, LlmError};

/// Responses API content part type for a prior turn replayed as `input`.
///
/// User/system turns are `input_text`. Assistant turns are previous model
/// output and MUST be `output_text` (`input_text` is rejected on those slots).
pub(crate) fn responses_content_part_type(role: ChatRole) -> &'static str {
    match role {
        ChatRole::Assistant => "output_text",
        ChatRole::System | ChatRole::User => "input_text",
    }
}

pub(crate) fn responses_content_part_type_for_role(role: &str) -> Result<&'static str, LlmError> {
    let parsed = ChatRole::try_from(role).map_err(LlmError::protocol)?;
    Ok(responses_content_part_type(parsed))
}

/// Classify an OpenAI-style error object from HTTP JSON into [`LlmError`].
pub(crate) fn classify_oai_error(error_obj: &serde_json::Value) -> Option<LlmError> {
    let msg = error_obj.get("message").and_then(|x| x.as_str())?;
    if msg.trim().is_empty() {
        return None;
    }
    let code = error_obj.get("code").and_then(|x| x.as_str()).unwrap_or("");
    let etype = error_obj.get("type").and_then(|x| x.as_str()).unwrap_or("");

    let is_permanent = code == "insufficient_quota"
        || code == "billing_hard_limit_reached"
        || code == "account_deactivated"
        || etype == "insufficient_quota"
        || code == "invalid_api_key"
        || (etype == "invalid_request_error" && code == "invalid_api_key");

    Some(if is_permanent {
        LlmError::Fatal(msg.to_string())
    } else if code == "rate_limit_exceeded"
        || etype == "rate_limit_error"
        || etype == "server_error"
        || code == "server_error"
    {
        LlmError::Transient(msg.to_string())
    } else {
        return None;
    })
}

pub(crate) fn llm_error_from_http(status: u16, body: &str) -> LlmError {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(error_obj) = v.get("error") {
            if let Some(err) = classify_oai_error(error_obj) {
                return err;
            }
        }
    }
    let snippet = react_core::text::truncate_chars_ellipsis(body, 500);
    match status {
        401 | 402 | 403 => LlmError::Fatal(format!("http {status}: {snippet}")),
        429 | 502 | 503 | 504 | 529 => LlmError::Transient(format!("http {status}: {snippet}")),
        s if (500..600).contains(&s) => LlmError::Transient(format!("http {s}: {snippet}")),
        _ => LlmError::Protocol(format!("http {status}: {snippet}")),
    }
}

pub(crate) fn llm_error_from_ureq(url: &str, err: ureq::Error) -> LlmError {
    let msg = format!("LLM request failed: {url}: {err}");
    if let ureq::Error::Transport(t) = &err {
        if let Some(io) = t.source().and_then(|s| s.downcast_ref::<std::io::Error>()) {
            if io.kind() == std::io::ErrorKind::TimedOut {
                return LlmError::Timeout(Duration::from_secs(30));
            }
        }
        return LlmError::Transient(msg);
    }
    LlmError::Protocol(msg)
}

pub(crate) fn ureq_send_json(
    agent: &ureq::Agent,
    method: &str,
    url: &str,
    extra_headers: &[(String, String)],
    bearer: Option<&str>,
    payload: serde_json::Value,
) -> Result<(u16, String), LlmError> {
    let mut r = agent
        .request(method, url)
        .set("Content-Type", "application/json");
    if let Some(k) = bearer {
        r = r.set("Authorization", &format!("Bearer {k}"));
    }
    for (h, v) in extra_headers {
        r = r.set(h, v);
    }
    match r.send_json(payload) {
        Ok(resp) => Ok((resp.status() as u16, resp.into_string().unwrap_or_default())),
        Err(ureq::Error::Status(s, rr)) => Ok((s as u16, rr.into_string().unwrap_or_default())),
        Err(e) => Err(llm_error_from_ureq(url, e)),
    }
}

pub(crate) fn ureq_get(
    agent: &ureq::Agent,
    url: &str,
    extra_headers: &[(String, String)],
    bearer: Option<&str>,
) -> Result<(u16, String), LlmError> {
    let mut r = agent.request("GET", url);
    if let Some(k) = bearer {
        r = r.set("Authorization", &format!("Bearer {k}"));
    }
    for (h, v) in extra_headers {
        r = r.set(h, v);
    }
    match r.call() {
        Ok(resp) => Ok((resp.status() as u16, resp.into_string().unwrap_or_default())),
        Err(ureq::Error::Status(s, rr)) => Ok((s as u16, rr.into_string().unwrap_or_default())),
        Err(e) => Err(llm_error_from_ureq(url, e)),
    }
}

/// Extract the model's text output from an OpenAI-style response.
///
/// When `structured` is true the caller expects a single JSON object (e.g. strict
/// JSON-schema mode). Providers can still split one logical assistant message across
/// multiple text parts, so preserve all non-empty chunks and let the schema parser
/// decide whether the joined text is valid.
pub(crate) fn extract_response_text(
    v: &serde_json::Value,
    structured: bool,
    http_status: u16,
    body_text: &str,
) -> Result<Option<String>, LlmError> {
    match v.get("error") {
        None | Some(serde_json::Value::Null) => {}
        Some(error_obj) => {
            return Err(classify_oai_error(error_obj)
                .unwrap_or_else(|| llm_error_from_http(http_status, body_text)));
        }
    }
    if let Some(s) = v.get("output_text").and_then(|x| x.as_str()) {
        if !s.trim().is_empty() {
            return Ok(Some(s.to_string()));
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
            "structured response contained {} output text items; joining chunks before schema parse",
            chunks.len()
        );
    }
    let joined = chunks.join("");
    if joined.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(joined))
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
    /// Passed through to `extract_response_text` so multipart structured
    /// provider payloads can be reassembled before schema parsing.
    pub structured: bool,
}

#[cfg(test)]
mod tests {
    use super::{
        extract_response_text, llm_error_from_http, responses_content_part_type,
        responses_content_part_type_for_role,
    };
    use react_core::llm::{ChatRole, LlmError};

    #[test]
    fn structured_response_text_joins_multiple_text_parts() {
        let response = serde_json::json!({
            "output": [{
                "content": [
                    {"type": "output_text", "text": "{\"type\":\"tool\","},
                    {"type": "output_text", "text": "\"name\":\"noop\"}"}
                ]
            }]
        });

        assert_eq!(
            extract_response_text(&response, true, 200, "{}")
                .expect("extract")
                .as_deref(),
            Some("{\"type\":\"tool\",\"name\":\"noop\"}")
        );
    }

    #[test]
    fn http_503_server_error_json_is_transient() {
        let err = llm_error_from_http(
            503,
            r#"{"error":{"message":"The server had an error","type":"server_error"}}"#,
        );
        assert!(err.retry(), "{err:?}");
        assert!(matches!(err, LlmError::Transient(_)), "{err:?}");
    }

    #[test]
    fn http_500_with_only_message_is_transient() {
        let err = llm_error_from_http(500, r#"{"error":{"message":"internal"}}"#);
        assert!(err.retry(), "{err:?}");
        assert!(matches!(err, LlmError::Transient(_)), "{err:?}");
    }

    #[test]
    fn http_400_with_message_is_protocol() {
        let err = llm_error_from_http(
            400,
            r#"{"error":{"message":"bad request","type":"invalid_request_error","code":"invalid_prompt"}}"#,
        );
        assert!(!err.retry(), "{err:?}");
        assert!(matches!(err, LlmError::Protocol(_)), "{err:?}");
    }

    #[test]
    fn insufficient_quota_is_fatal_even_on_429() {
        let err = llm_error_from_http(
            429,
            r#"{"error":{"message":"you exceeded your current quota","code":"insufficient_quota"}}"#,
        );
        assert!(!err.retry(), "{err:?}");
        assert!(matches!(err, LlmError::Fatal(_)), "{err:?}");
    }

    #[test]
    fn extract_unclassified_error_uses_http_status() {
        let v = serde_json::json!({"error":{"message":"internal"}});
        let body = v.to_string();
        let err = extract_response_text(&v, false, 503, &body).expect_err("error envelope");
        assert!(err.retry(), "{err:?}");
        assert!(matches!(err, LlmError::Transient(_)), "{err:?}");

        let err = extract_response_text(&v, false, 200, &body).expect_err("error envelope");
        assert!(!err.retry(), "{err:?}");
        assert!(matches!(err, LlmError::Protocol(_)), "{err:?}");
    }

    #[test]
    fn completed_responses_payload_with_null_error_extracts_text() {
        let response = serde_json::json!({
            "id": "resp_luna_high",
            "object": "response",
            "status": "completed",
            "background": true,
            "error": null,
            "output": [
                {"type": "reasoning", "id": "rs_1", "summary": []},
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "{\"type\":\"tool\",\"name\":\"catalog\"}"}
                    ]
                }
            ],
            "output_text": "{\"type\":\"tool\",\"name\":\"catalog\"}"
        });
        let body = response.to_string();
        assert_eq!(
            extract_response_text(&response, true, 200, &body)
                .expect("null error is not an envelope")
                .as_deref(),
            Some("{\"type\":\"tool\",\"name\":\"catalog\"}")
        );
    }

    #[test]
    fn completed_responses_without_output_text_uses_message_content() {
        let response = serde_json::json!({
            "status": "completed",
            "background": true,
            "error": null,
            "output": [
                {"type": "reasoning", "id": "rs_1", "summary": []},
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "{\"ok\":true}"}
                    ]
                }
            ]
        });
        let body = response.to_string();
        assert_eq!(
            extract_response_text(&response, true, 200, &body)
                .expect("message content")
                .as_deref(),
            Some("{\"ok\":true}")
        );
    }

    #[test]
    fn responses_error_string_is_fail_closed() {
        let response = serde_json::json!({
            "status": "failed",
            "error": "upstream timeout"
        });
        let body = response.to_string();
        let err = extract_response_text(&response, false, 200, &body).expect_err("string error");
        assert!(matches!(err, LlmError::Protocol(_)), "{err:?}");
    }

    #[test]
    fn responses_content_part_type_is_role_exhaustive() {
        assert_eq!(responses_content_part_type(ChatRole::User), "input_text");
        assert_eq!(responses_content_part_type(ChatRole::System), "input_text");
        assert_eq!(
            responses_content_part_type(ChatRole::Assistant),
            "output_text"
        );
        assert_eq!(
            responses_content_part_type_for_role("assistant").expect("role"),
            "output_text"
        );
        assert!(responses_content_part_type_for_role("tool").is_err());
    }
}
