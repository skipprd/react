#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmProviderType;
    use httpmock::prelude::*;

    #[tokio::test]
    async fn chat_and_embed_shape() {
        let server = MockServer::start();
        let chat_mock = server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(200)
                .header("content-type", "application/json")
                .json_body_obj(&serde_json::json!({
                    "choices": [ { "message": {"role": "assistant", "content": "hi"} } ]
                }));
        });
        let emb_mock = server.mock(|when, then| {
            when.method(POST).path("/v1/embeddings");
            then.status(200)
                .header("content-type", "application/json")
                .json_body_obj(&serde_json::json!({
                    "data": [ { "embedding": [0.1, 0.2] }, { "embedding": [0.3, 0.4] } ]
                }));
        });

        let cfg = LlmConfig {
            provider: LlmProviderType::OpenAICompat,
            reason_model: Some("gpt-test".to_string()),
            task_model: None,
            embed_model: Some("text-emb".to_string()),
            base_url: Some(server.base_url()),
            api_key: Some("x".to_string()),
            gpu_layers: None,
            context_length: Some(1024),
        };
        let llm = OpenAICompatModel::new(cfg);
        let out = llm
            .chat(
                &[ChatMessage {
                    role: react_core::llm::ChatRole::User,
                    content: "hello".into(),
                }],
                &react_core::llm::LlmCallOptions {
                    prompt_id: "react.openai_compat.tests.chat",
                    thread_id: None,
                    expected_format: react_core::llm::LlmExpectedFormat::Text,
                    max_output_tokens: None,
                    temperature: None,
                    top_p: None,
                    reasoning_effort: None,
                    timeout_secs: None,
                    model: None,
                },
            )
            .await
            .unwrap();
        assert!(out.contains("hi"));
        let emb = llm.embed(&vec!["a".into(), "b".into()]).await.unwrap();
        assert_eq!(emb.len(), 2);
        assert_eq!(emb[0].len(), 2);

        chat_mock.assert();
        emb_mock.assert();
    }

    #[tokio::test]
    async fn gpt5_responses_400_does_not_fallback_to_chat_completions() {
        let server = MockServer::start();
        let responses = server.mock(|when, then| {
            when.method(POST).path("/v1/responses");
            then.status(400)
                .header("content-type", "application/json")
                .body(r#"{"error":{"message":"bad schema","type":"invalid_request_error"}}"#);
        });
        let chat = server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(200)
                .header("content-type", "application/json")
                .json_body_obj(&serde_json::json!({
                    "choices": [ { "message": {"role": "assistant", "content": "should not run"} } ]
                }));
        });

        let cfg = LlmConfig {
            provider: LlmProviderType::OpenAICompat,
            reason_model: Some("gpt-5.6-luna".to_string()),
            task_model: None,
            embed_model: Some("text-emb".to_string()),
            base_url: Some(server.base_url()),
            api_key: Some("x".to_string()),
            gpu_layers: None,
            context_length: Some(1024),
        };
        let llm = OpenAICompatModel::new(cfg);
        let err = llm
            .chat(
                &[ChatMessage {
                    role: react_core::llm::ChatRole::User,
                    content: "hello".into(),
                }],
                &react_core::llm::LlmCallOptions {
                    prompt_id: "react.openai_compat.tests.no_fallback",
                    thread_id: None,
                    expected_format: react_core::llm::LlmExpectedFormat::Text,
                    max_output_tokens: None,
                    temperature: None,
                    top_p: None,
                    reasoning_effort: None,
                    timeout_secs: None,
                    model: None,
                },
            )
            .await
            .expect_err("400 must be LlmError");
        assert!(
            matches!(err, react_core::llm::LlmError::Protocol(_)),
            "{err:?}"
        );
        responses.assert();
        chat.assert_hits(0);
    }

    #[tokio::test]
    async fn responses_api_rejects_empty_messages() {
        let cfg = LlmConfig {
            provider: LlmProviderType::OpenAICompat,
            reason_model: Some("gpt-5.6-luna".to_string()),
            task_model: None,
            embed_model: Some("text-emb".to_string()),
            base_url: Some("http://127.0.0.1:1".to_string()),
            api_key: Some("x".to_string()),
            gpu_layers: None,
            context_length: Some(1024),
        };
        let llm = OpenAICompatModel::new(cfg);
        let err = llm
            .chat(
                &[],
                &react_core::llm::LlmCallOptions {
                    prompt_id: "react.openai_compat.tests.empty_messages",
                    thread_id: None,
                    expected_format: react_core::llm::LlmExpectedFormat::Text,
                    max_output_tokens: None,
                    temperature: None,
                    top_p: None,
                    reasoning_effort: None,
                    timeout_secs: None,
                    model: None,
                },
            )
            .await
            .expect_err("empty messages must be protocol");
        match err {
            react_core::llm::LlmError::Protocol(msg) => {
                assert!(msg.contains("no messages"), "{msg}");
            }
            other => panic!("expected protocol, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn chat_completions_rejects_empty_messages() {
        let cfg = LlmConfig {
            provider: LlmProviderType::OpenAICompat,
            reason_model: Some("gpt-4o".to_string()),
            task_model: None,
            embed_model: Some("text-emb".to_string()),
            base_url: Some("http://127.0.0.1:1".to_string()),
            api_key: Some("x".to_string()),
            gpu_layers: None,
            context_length: Some(1024),
        };
        let llm = OpenAICompatModel::new(cfg);
        let err = llm
            .chat(
                &[],
                &react_core::llm::LlmCallOptions {
                    prompt_id: "react.openai_compat.tests.empty_messages_chat",
                    thread_id: None,
                    expected_format: react_core::llm::LlmExpectedFormat::Text,
                    max_output_tokens: None,
                    temperature: None,
                    top_p: None,
                    reasoning_effort: None,
                    timeout_secs: None,
                    model: None,
                },
            )
            .await
            .expect_err("empty messages must be protocol");
        match err {
            react_core::llm::LlmError::Protocol(msg) => {
                assert!(msg.contains("no messages"), "{msg}");
            }
            other => panic!("expected protocol, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn http_429_is_transient_llm_error() {
        let server = MockServer::start();
        let chat = server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(429)
                .header("content-type", "application/json")
                .body(r#"{"error":{"message":"rate","type":"rate_limit_error","code":"rate_limit_exceeded"}}"#);
        });

        let cfg = LlmConfig {
            provider: LlmProviderType::OpenAICompat,
            reason_model: Some("gpt-4o".to_string()),
            task_model: None,
            embed_model: Some("text-emb".to_string()),
            base_url: Some(server.base_url()),
            api_key: Some("x".to_string()),
            gpu_layers: None,
            context_length: Some(1024),
        };
        let llm = OpenAICompatModel::new(cfg);
        let err = llm
            .chat(
                &[ChatMessage {
                    role: react_core::llm::ChatRole::User,
                    content: "hello".into(),
                }],
                &react_core::llm::LlmCallOptions {
                    prompt_id: "react.openai_compat.tests.throttle",
                    thread_id: None,
                    expected_format: react_core::llm::LlmExpectedFormat::Text,
                    max_output_tokens: None,
                    temperature: None,
                    top_p: None,
                    reasoning_effort: None,
                    timeout_secs: None,
                    model: None,
                },
            )
            .await
            .expect_err("429 must be LlmError");
        assert!(err.retry(), "{err:?}");
        chat.assert_hits(1);
    }

    #[test]
    fn defaults_base_url_to_openai_compat_endpoint() {
        let llm = OpenAICompatModel::new(LlmConfig {
            provider: LlmProviderType::OpenAICompat,
            base_url: None,
            reason_model: Some("gpt-test".to_string()),
            task_model: None,
            embed_model: Some("text-emb".to_string()),
            api_key: None,
            gpu_layers: None,
            context_length: Some(1024),
        });
        assert_eq!(
            llm.base_url(),
            crate::config::DEFAULT_OPENAI_COMPAT_BASE_URL
        );
    }
}
use super::types::{
    extract_response_text, llm_error_from_http, pretty_json, ureq_send_json, OaiChatMessage,
    OaiChatReq, OaiChatResp, OaiEmbReq, OaiEmbResp,
};
use super::{ChatMessage, LargeLanguageModel, LlmConfig};

fn pretty_val(val: &serde_json::Value) -> String {
    serde_json::to_string_pretty(val).unwrap_or_else(|_| val.to_string())
}

/// Minimal OpenAI-compatible HTTP provider (blocking, no Tokio runtime required).
#[derive(Clone)]
pub struct OpenAICompatModel {
    cfg: LlmConfig,
    agent: ureq::Agent,
}

impl OpenAICompatModel {
    pub fn new(cfg: LlmConfig) -> Self {
        let http_timeout_secs: u64 =
            crate::runtime_settings::getenv("LLM_HTTP_TIMEOUT_SECS", "1200")
                .parse()
                .unwrap_or(1200);
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(http_timeout_secs))
            .build();
        Self { cfg, agent }
    }

    fn base_url(&self) -> String {
        self.cfg
            .base_url
            .clone()
            .unwrap_or_else(|| crate::config::DEFAULT_OPENAI_COMPAT_BASE_URL.to_string())
    }

    fn post_json(
        &self,
        url: &str,
        payload: serde_json::Value,
    ) -> Result<(u16, String), react_core::llm::LlmError> {
        ureq_send_json(
            &self.agent,
            "POST",
            url,
            &[],
            self.cfg.api_key.as_deref(),
            payload,
        )
    }
}

impl OpenAICompatModel {
    fn chat_sync(
        &self,
        messages: &[ChatMessage],
        options: &react_core::llm::LlmCallOptions,
    ) -> Result<String, react_core::llm::LlmError> {
        react_core::llm::require_nonempty_messages(messages)?;
        let base = self.base_url();
        let model = options
            .model
            .clone()
            .or_else(|| self.cfg.reason_model.clone())
            .ok_or_else(|| react_core::llm::LlmError::protocol("missing reason_model"))?;
        let use_responses =
            react_core::llm::openai_api_for_model(&model) == react_core::llm::OpenAiApi::Responses;
        if use_responses {
            #[derive(serde::Serialize)]
            struct RespPart {
                #[serde(rename = "type")]
                r#type: String,
                text: String,
            }
            #[derive(serde::Serialize)]
            struct RespMsg {
                role: String,
                content: Vec<RespPart>,
            }
            #[derive(serde::Serialize)]
            struct RespReasoning {
                effort: String, // none|low|medium|high
            }
            #[derive(serde::Serialize)]
            struct RespReq {
                model: String,
                // Use 'input' per Responses API, with typed content parts ('input_text')
                input: Vec<RespMsg>,
                #[serde(skip_serializing_if = "Option::is_none")]
                modalities: Option<Vec<String>>,
                #[serde(skip_serializing_if = "Option::is_none")]
                response_format: Option<serde_json::Value>,
                #[serde(skip_serializing_if = "Option::is_none")]
                max_output_tokens: Option<i32>,
                #[serde(skip_serializing_if = "Option::is_none")]
                reasoning: Option<RespReasoning>,
            }
            let url = format!("{}/v1/responses", base.trim_end_matches('/'));
            let max_tokens_env: i32 = crate::runtime_settings::getenv("LLM_MAX_TOKENS", "8192")
                .parse()
                .unwrap_or(8192);
            let max_tokens: i32 = options
                .max_output_tokens
                .map(|v| v as i32)
                .unwrap_or(max_tokens_env);
            // Map messages to Responses 'input' with typed content parts
            let mut msgs: Vec<RespMsg> = Vec::new();
            for m in messages.iter() {
                let part = RespPart {
                    r#type: crate::llm::types::responses_content_part_type(m.role).to_string(),
                    text: m.content.clone(),
                };
                msgs.push(RespMsg {
                    role: m.role.to_string(),
                    content: vec![part],
                });
            }
            let response_format = match &options.expected_format {
                react_core::llm::LlmExpectedFormat::Text => serde_json::json!({ "type": "text" }),
                react_core::llm::LlmExpectedFormat::JsonObject => {
                    serde_json::json!({ "type": "json_object" })
                }
                react_core::llm::LlmExpectedFormat::JsonSchema(schema) => serde_json::json!({
                    "type": "json_schema",
                    "name": schema.name().replace('.', "_"),
                    "schema": schema.schema(),
                    "strict": true
                }),
            };
            let body = RespReq {
                model: model.clone(),
                input: msgs,
                modalities: Some(vec!["text".to_string()]),
                response_format: Some(response_format),
                max_output_tokens: Some(max_tokens),
                reasoning: Some(RespReasoning {
                    effort: match options
                        .reasoning_effort
                        .unwrap_or(react_core::llm::ReasoningEffort::Low)
                    {
                        react_core::llm::ReasoningEffort::None => "none",
                        react_core::llm::ReasoningEffort::Low => "low",
                        react_core::llm::ReasoningEffort::Medium => "medium",
                        react_core::llm::ReasoningEffort::High
                        | react_core::llm::ReasoningEffort::ExtraHigh => "high",
                    }
                    .to_string(),
                }),
            };
            let payload = serde_json::to_value(&body)
                .map_err(|e| react_core::llm::LlmError::protocol(e.to_string()))?;
            tracing::debug!(
                "LLM(responses) request model='{}'\n{}",
                model,
                pretty_val(&payload)
            );
            let (status, body_text) = self.post_json(&url, payload)?;
            if !(200..300).contains(&status) {
                tracing::debug!(
                    "LLM(responses) non-2xx status={} body:\n{}",
                    status,
                    pretty_json(&body_text)
                );
                return Err(llm_error_from_http(status, &body_text));
            }
            tracing::debug!("LLM(responses) response:\n{}", pretty_json(&body_text));

            let v: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| react_core::llm::LlmError::protocol(e.to_string()))?;
            let structured = matches!(
                options.expected_format,
                react_core::llm::LlmExpectedFormat::JsonSchema(_)
            );
            match extract_response_text(&v, structured, status, &body_text)? {
                Some(t) => {
                    tracing::debug!("LLM(responses) extracted text:\n{}", t);
                    return Ok(t);
                }
                None => {}
            }
            let snippet = react_core::text::truncate_chars_ellipsis(&body_text, 1200);
            Err(react_core::llm::LlmError::protocol(format!(
                "empty response: {snippet}"
            )))
        } else {
            let url = format!("{}/v1/chat/completions", base.trim_end_matches('/'));
            // latency-optimized defaults
            let max_tokens: u32 = crate::runtime_settings::getenv("LLM_MAX_TOKENS", "1024")
                .parse()
                .unwrap_or(1024);
            let temperature: f32 = crate::runtime_settings::getenv("LLM_TEMPERATURE", "0.2")
                .parse()
                .unwrap_or(0.2);
            let top_p: f32 = crate::runtime_settings::getenv("LLM_TOP_P", "1.0")
                .parse()
                .unwrap_or(1.0);
            let body = OaiChatReq {
                model,
                messages: messages
                    .iter()
                    .map(|m| OaiChatMessage {
                        role: m.role.to_string(),
                        content: m.content.clone(),
                    })
                    .collect(),
                stream: Some(false),
                max_tokens: Some(max_tokens),
                temperature: Some(temperature),
                top_p: Some(top_p),
                response_format: None,
            };
            let payload = serde_json::to_value(&body)
                .map_err(|e| react_core::llm::LlmError::protocol(e.to_string()))?;
            tracing::debug!(
                "LLM(chat.completions) request model='{}'\n{}",
                body.model,
                pretty_val(&payload)
            );
            let (status, body_text) = self.post_json(&url, payload)?;
            if !(200..300).contains(&status) {
                tracing::debug!(
                    "LLM(chat.completions) non-2xx status={} body:\n{}",
                    status,
                    pretty_json(&body_text)
                );
                return Err(llm_error_from_http(status, &body_text));
            }
            tracing::debug!(
                "LLM(chat.completions) response:\n{}",
                pretty_json(&body_text)
            );
            let obj: OaiChatResp = serde_json::from_str(&body_text)
                .map_err(|e| react_core::llm::LlmError::protocol(e.to_string()))?;
            let mut out = String::new();
            for c in obj.choices.iter() {
                if let Some(m) = &c.message {
                    out.push_str(&m.content);
                }
                if let Some(d) = &c.delta {
                    if let Some(s) = &d.content {
                        out.push_str(s);
                    }
                }
            }
            tracing::debug!("LLM(chat.completions) text:\n{}", out);
            Ok(out)
        }
    }
    fn embed_sync(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, react_core::llm::LlmError> {
        let base = self.base_url();
        let model = self
            .cfg
            .embed_model
            .clone()
            .ok_or_else(|| react_core::llm::LlmError::protocol("missing embed_model"))?;
        let url = format!("{}/v1/embeddings", base.trim_end_matches('/'));
        // Debug: pretty-print embedding inputs
        tracing::debug!(
            "LLM(embeddings) request model='{}' inputs:\n{}",
            model,
            pretty_json(&serde_json::to_string(texts).unwrap_or_else(|_| "[]".to_string()))
        );
        let body = OaiEmbReq {
            model,
            input: texts.to_vec(),
        };
        let payload = serde_json::to_value(&body)
            .map_err(|e| react_core::llm::LlmError::protocol(e.to_string()))?;
        let (status, body_text) = self.post_json(&url, payload)?;
        if !(200..300).contains(&status) {
            tracing::debug!(
                "LLM(embeddings) non-2xx status={} body:\n{}",
                status,
                pretty_json(&body_text)
            );
            return Err(llm_error_from_http(status, &body_text));
        }
        // Do not log raw embedding vectors; parse silently and report only shape
        let obj: OaiEmbResp = serde_json::from_str(&body_text)
            .map_err(|e| react_core::llm::LlmError::protocol(e.to_string()))?;
        let vecs: Vec<Vec<f32>> = obj.data.into_iter().map(|d| d.embedding).collect();
        let dim = vecs.get(0).map(|v| v.len()).unwrap_or(0);
        tracing::debug!("LLM(embeddings) shape: count={} dim={}", vecs.len(), dim);
        Ok(vecs)
    }
}

#[async_trait::async_trait]
impl LargeLanguageModel for OpenAICompatModel {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        options: &react_core::llm::LlmCallOptions,
    ) -> Result<String, react_core::llm::LlmError> {
        let this = self.clone();
        let messages = messages.to_vec();
        let options = options.clone();
        tokio::task::spawn_blocking(move || this.chat_sync(&messages, &options))
            .await
            .map_err(|e| react_core::llm::LlmError::protocol(format!("join: {e}")))?
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, react_core::llm::LlmError> {
        let this = self.clone();
        let texts = texts.to_vec();
        tokio::task::spawn_blocking(move || this.embed_sync(&texts))
            .await
            .map_err(|e| react_core::llm::LlmError::protocol(format!("join: {e}")))?
    }
}
