use crate::llm::adapter::Adapter;
use crate::llm::types::*;
use react_core::llm::LlmError;
use serde::Serialize;

pub struct OpenAIResponsesAdapter;
impl OpenAIResponsesAdapter {
    pub fn new() -> Self {
        Self {}
    }
}

fn responses_supports_sampling_controls(model: &str) -> bool {
    react_core::llm::openai_api_for_model(model) == react_core::llm::OpenAiApi::ChatCompletions
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

    fn build_chat_http(&self, req: &ChatRequest) -> Result<ProviderHttpRequest, LlmError> {
        react_core::llm::require_nonempty_messages(&req.messages)?;
        let mut msgs: Vec<RespMsg> = Vec::new();
        for m in req.messages.iter() {
            let part = RespPart {
                r#type: responses_content_part_type_for_role(&m.role)?.to_string(),
                text: m.content.clone(),
            };
            msgs.push(RespMsg {
                role: m.role.to_string(),
                content: vec![part],
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
            body: serde_json::to_value(body).map_err(|e| LlmError::protocol(e.to_string()))?,
        })
    }

    fn parse_chat_http(&self, resp: &ProviderHttpResponse) -> Result<ChatResponse, LlmError> {
        let v: serde_json::Value =
            serde_json::from_str(&resp.body_text).map_err(|e| LlmError::protocol(e.to_string()))?;
        let raw = Some(v.clone());
        match extract_response_text(&v, resp.structured, resp.status, &resp.body_text)? {
            Some(t) => Ok(ChatResponse { text: t, raw }),
            None => {
                let snippet = react_core::text::truncate_chars_ellipsis(&resp.body_text, 1200);
                Err(LlmError::protocol(format!("empty response: {snippet}")))
            }
        }
    }

    fn build_embed_http(&self, req: &EmbedRequest) -> Result<ProviderHttpRequest, LlmError> {
        let body = OaiEmbReq {
            model: req.model.clone(),
            input: req.inputs.clone(),
        };
        Ok(ProviderHttpRequest {
            method: "POST".to_string(),
            url: "/v1/embeddings".to_string(),
            headers: vec![],
            body: serde_json::to_value(body).map_err(|e| LlmError::protocol(e.to_string()))?,
        })
    }

    fn parse_embed_http(&self, resp: &ProviderHttpResponse) -> Result<EmbedResponse, LlmError> {
        let obj: OaiEmbResp =
            serde_json::from_str(&resp.body_text).map_err(|e| LlmError::protocol(e.to_string()))?;
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
    fn build_chat_http_uses_output_text_for_assistant_turns() {
        let ad = OpenAIResponsesAdapter::new();
        let req = ChatRequest {
            model: "gpt-5.6-luna".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "rules".to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: "hi".to_string(),
                },
                ChatMessage {
                    role: "assistant".to_string(),
                    content: "{\"type\":\"tool\"}".to_string(),
                },
            ],
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
        let input = http
            .body
            .get("input")
            .and_then(|x| x.as_array())
            .expect("input");
        assert_eq!(input[0]["content"][0]["type"].as_str(), Some("input_text"));
        assert_eq!(input[1]["content"][0]["type"].as_str(), Some("input_text"));
        assert_eq!(input[2]["content"][0]["type"].as_str(), Some("output_text"));
        assert_eq!(input[2]["role"].as_str(), Some("assistant"));
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

    #[test]
    fn build_chat_http_sends_luna_high_reasoning() {
        let ad = OpenAIResponsesAdapter::new();
        let req = ChatRequest {
            model: "gpt-5.6-luna".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            max_output_tokens: None,
            temperature: Some(0.2),
            top_p: Some(1.0),
            response_format: None,
            reasoning_effort: Some("high".to_string()),
            prompt_id: None,
            thread_id: None,
            execution_mode: None,
        };
        let http = ad.build_chat_http(&req).expect("build");
        assert_eq!(http.url, "/v1/responses");
        assert_eq!(
            http.body.get("model").and_then(|x| x.as_str()),
            Some("gpt-5.6-luna")
        );
        assert_eq!(
            http.body
                .get("reasoning")
                .and_then(|r| r.get("effort"))
                .and_then(|x| x.as_str()),
            Some("high")
        );
        assert!(http.body.get("temperature").is_none());
        assert!(http.body.get("top_p").is_none());
    }

    #[test]
    fn build_chat_http_rejects_empty_messages() {
        let ad = OpenAIResponsesAdapter::new();
        let req = ChatRequest {
            model: "gpt-5.6-luna".to_string(),
            messages: vec![],
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            response_format: None,
            reasoning_effort: None,
            prompt_id: None,
            thread_id: None,
            execution_mode: None,
        };
        let err = ad.build_chat_http(&req).expect_err("empty");
        match err {
            LlmError::Protocol(msg) => assert!(msg.contains("no messages"), "{msg}"),
            other => panic!("expected protocol, got {other:?}"),
        }
    }

    #[test]
    fn parse_chat_http_accepts_completed_luna_payload_with_null_error() {
        let ad = OpenAIResponsesAdapter::new();
        let body = serde_json::json!({
            "id": "resp_luna_high",
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
            ]
        })
        .to_string();
        let parsed = ad
            .parse_chat_http(&ProviderHttpResponse {
                status: 200,
                body_text: body,
                structured: true,
            })
            .expect("parse");
        assert_eq!(parsed.text, "{\"type\":\"tool\",\"name\":\"catalog\"}");
    }
}
