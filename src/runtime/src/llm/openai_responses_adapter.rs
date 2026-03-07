use crate::llm::adapter::Adapter;
use crate::llm::types::*;
use serde::Serialize;

pub struct OpenAIResponsesAdapter;
impl OpenAIResponsesAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

fn responses_supports_sampling_controls(model: &str) -> bool {
    // Some newer "reasoning" model families reject temperature/top_p on the Responses API.
    // Example: gpt-5.1 returns `Unsupported parameter: 'temperature' is not supported with this model.`
    //
    // When unsupported, omit the parameters entirely rather than failing the request.
    let m = model.trim();
    !(m.starts_with("gpt-5") || m.starts_with("o"))
}

#[derive(Serialize)]
struct RespPart {
    #[serde(rename = "type")]
    r#type: String,
    text: String,
}
#[derive(Serialize)]
struct RespMsg {
    role: String,
    content: Vec<RespPart>,
}
#[derive(Serialize)]
struct RespReq {
    model: String,
    input: Vec<RespMsg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<RespText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    /// Optional reasoning effort hint (Responses API).
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<RespReasoning>,
    /// OpenAI Responses background execution mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    store: Option<bool>,
}
#[derive(Serialize)]
struct RespReasoning {
    effort: String, // none|low|medium|high
}
#[derive(Serialize)]
struct RespText {
    /// The OpenAI Responses API `text.format` object.
    ///
    /// Examples:
    /// - `{ "type": "text" }`
    /// - `{ "type": "json_object" }`
    /// - `{ "type": "json_schema", "name": "...", "schema": {...}, "strict": true }`
    format: serde_json::Value,
}

impl Adapter for OpenAIResponsesAdapter {
    fn capabilities(&self, _model: &str) -> Capabilities {
        Capabilities {
            supports_responses_api: true,
            supports_stream: false,
            context_window: 128_000,
            embed_input_tokens: 8_000,
        }
    }

    fn build_chat_http(&self, req: &ChatRequest) -> Result<ProviderHttpRequest, String> {
        let mut msgs: Vec<RespMsg> = Vec::new();
        for m in req.messages.iter() {
            let part = RespPart {
                r#type: "input_text".to_string(),
                text: m.content.clone(),
            };
            msgs.push(RespMsg {
                role: m.role.to_string(),
                content: vec![part],
            });
        }
        if msgs.is_empty() {
            msgs.push(RespMsg {
                role: "user".to_string(),
                content: vec![RespPart {
                    r#type: "input_text".to_string(),
                    text: String::new(),
                }],
            });
        }

        // Default to plain text, but allow callers to request structured output via `response_format`.
        // This maps directly to the Responses API `text.format` object.
        let format = match req.response_format.as_ref() {
            None | Some(ChatResponseFormat::Text) => serde_json::json!({"type":"text"}),
            Some(ChatResponseFormat::JsonObject) => serde_json::json!({"type":"json_object"}),
            Some(ChatResponseFormat::JsonSchema {
                name,
                schema,
                strict,
            }) => serde_json::json!({
                "type": "json_schema",
                "name": name,
                "schema": schema,
                "strict": strict,
            }),
        };

        let body = RespReq {
            model: req.model.clone(),
            input: msgs,
            text: Some(RespText { format }),
            max_output_tokens: req.max_output_tokens.map(|v| v as i32),
            temperature: if responses_supports_sampling_controls(&req.model) {
                req.temperature
            } else {
                None
            },
            top_p: if responses_supports_sampling_controls(&req.model) {
                req.top_p
            } else {
                None
            },
            reasoning: req
                .reasoning_effort
                .as_ref()
                .map(|s| RespReasoning { effort: s.clone() }),
            background: match req.execution_mode {
                Some(LlmExecutionMode::BackgroundPreferred) => Some(true),
                _ => None,
            },
            store: match req.execution_mode {
                Some(LlmExecutionMode::BackgroundPreferred) => Some(true),
                _ => None,
            },
        };
        Ok(ProviderHttpRequest {
            method: "POST".to_string(),
            url: "/v1/responses".to_string(),
            headers: vec![],
            body: serde_json::to_value(body).map_err(|e| e.to_string())?,
        })
    }

    fn parse_chat_http(&self, resp: &ProviderHttpResponse) -> Result<ChatResponse, String> {
        let v: serde_json::Value =
            serde_json::from_str(&resp.body_text).map_err(|e| e.to_string())?;
        let raw = Some(v.clone());
        if let Some(t) = extract_response_text(&v) {
            return Ok(ChatResponse { text: t, raw });
        }
        let snippet = if resp.body_text.len() > 1200 {
            format!("{}...", &resp.body_text[..1200])
        } else {
            resp.body_text.clone()
        };
        Err(format!("empty response: {}", snippet))
    }

    fn build_embed_http(&self, req: &EmbedRequest) -> Result<ProviderHttpRequest, String> {
        let body = OaiEmbReq {
            model: req.model.clone(),
            input: req.inputs.clone(),
        };
        Ok(ProviderHttpRequest {
            method: "POST".to_string(),
            url: "/v1/embeddings".to_string(),
            headers: vec![],
            body: serde_json::to_value(body).map_err(|e| e.to_string())?,
        })
    }

    fn parse_embed_http(&self, resp: &ProviderHttpResponse) -> Result<EmbedResponse, String> {
        let obj: OaiEmbResp = serde_json::from_str(&resp.body_text).map_err(|e| e.to_string())?;
        let vecs: Vec<Vec<f32>> = obj.data.into_iter().map(|d| d.embedding).collect();
        let dim = vecs.get(0).map(|v| v.len()).unwrap_or(0);
        Ok(EmbedResponse { vectors: vecs, dim })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_chat_http_passes_response_format_to_text_format() {
        let ad = OpenAIResponsesAdapter::new();
        let req = ChatRequest {
            model: "gpt-4.1-mini".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            response_format: Some(ChatResponseFormat::JsonObject),
            reasoning_effort: None,
            prompt_id: None,
            thread_id: None,
            execution_mode: None,
        };
        let http = ad.build_chat_http(&req).expect("build");
        assert_eq!(http.url, "/v1/responses");
        let fmt = http
            .body
            .get("text")
            .and_then(|t| t.get("format"))
            .and_then(|f| f.get("type"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        assert_eq!(fmt, "json_object");
    }

    #[test]
    fn build_chat_http_supports_json_schema_response_format() {
        let ad = OpenAIResponsesAdapter::new();
        let req = ChatRequest {
            model: "gpt-4.1-mini".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            response_format: Some(ChatResponseFormat::JsonSchema {
                name: "agent.step.v1".to_string(),
                schema: serde_json::json!({"type":"object"}),
                strict: true,
            }),
            reasoning_effort: None,
            prompt_id: None,
            thread_id: None,
            execution_mode: None,
        };
        let http = ad.build_chat_http(&req).expect("build");
        let fmt = http
            .body
            .get("text")
            .and_then(|t| t.get("format"))
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        assert_eq!(
            fmt.get("type").and_then(|x| x.as_str()),
            Some("json_schema")
        );
        assert_eq!(
            fmt.get("name").and_then(|x| x.as_str()),
            Some("agent.step.v1")
        );
        assert_eq!(fmt.get("strict").and_then(|x| x.as_bool()), Some(true));
        assert_eq!(
            fmt.get("schema")
                .and_then(|x| x.get("type"))
                .and_then(|x| x.as_str()),
            Some("object")
        );
    }

    #[test]
    fn build_chat_http_defaults_to_text_format() {
        let ad = OpenAIResponsesAdapter::new();
        let req = ChatRequest {
            model: "gpt-4.1-mini".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            response_format: None,
            reasoning_effort: None,
            prompt_id: None,
            thread_id: None,
            execution_mode: None,
        };
        let http = ad.build_chat_http(&req).expect("build");
        let fmt = http
            .body
            .get("text")
            .and_then(|t| t.get("format"))
            .and_then(|f| f.get("type"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        assert_eq!(fmt, "text");
    }

    #[test]
    fn build_chat_http_enables_background_when_requested() {
        let ad = OpenAIResponsesAdapter::new();
        let req = ChatRequest {
            model: "gpt-5.2".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            response_format: None,
            reasoning_effort: None,
            prompt_id: None,
            thread_id: None,
            execution_mode: Some(LlmExecutionMode::BackgroundPreferred),
        };
        let http = ad.build_chat_http(&req).expect("build");
        assert_eq!(
            http.body.get("background").and_then(|x| x.as_bool()),
            Some(true)
        );
        assert_eq!(http.body.get("store").and_then(|x| x.as_bool()), Some(true));
    }
}
