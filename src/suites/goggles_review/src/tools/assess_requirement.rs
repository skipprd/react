use async_trait::async_trait;
use serde_json::Value;

use react_core::agent::AgentCtx;
use react_core::llm::{ChatMessage, ChatRole, LlmCallOptions, LlmExpectedFormat, ReasoningEffort};
use react_core::tools::Tool;

pub struct AssessRequirementTool;

const ASSESSMENT_PROMPT: &str = r#"You are a legal document reviewer assessing whether uploaded evidence satisfies a specific requirement for a bridging loan application.

You will be given:
1. The REQUIREMENT that needs to be satisfied
2. The QUESTION CONTEXT explaining what was asked
3. The EXTRACTED TEXT from the uploaded document

Your task:
- Determine if the document satisfies the requirement
- Quote verbatim passages from the document that support your conclusion
- Provide a confidence score (0.0 to 1.0)
- Explain your reasoning

Respond with valid JSON in this exact format:
{
  "verdict": "satisfied" | "not_satisfied" | "inconclusive" | "needs_human_review",
  "confidence": 0.85,
  "reasoning": "Brief explanation of your assessment",
  "citations": [
    {
      "text": "Exact verbatim quote from the document",
      "page": 1,
      "paragraph": null,
      "relevance_note": "Why this passage is relevant to the requirement"
    }
  ]
}

Rules:
- Use "satisfied" only when the document clearly meets the requirement
- Use "not_satisfied" when the document clearly does NOT meet the requirement
- Use "inconclusive" when the document is ambiguous or partially meets it
- Use "needs_human_review" when you cannot determine from text alone (e.g. signatures, stamps, image quality)
- Citations MUST be verbatim quotes from the extracted text
- Be conservative: when in doubt, use "needs_human_review"
"#;

#[async_trait]
impl Tool for AssessRequirementTool {
    fn name(&self) -> &'static str {
        "assess_requirement"
    }

    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String> {
        let extracted_text = args
            .get("extracted_text")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let requirement_label = args
            .get("requirement_label")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown requirement");
        let requirement_description = args
            .get("requirement_description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let question_prompt = args
            .get("question_prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if extracted_text.trim().is_empty() {
            return Ok(serde_json::json!({
                "verdict": "needs_human_review",
                "confidence": 0.0,
                "reasoning": "No text could be extracted from the document. Manual review required.",
                "citations": []
            }));
        }

        let user_message = format!(
            "REQUIREMENT: {}\nDESCRIPTION: {}\nQUESTION CONTEXT: {}\n\nEXTRACTED TEXT:\n{}",
            requirement_label, requirement_description, question_prompt, extracted_text
        );

        let messages = vec![
            ChatMessage {
                role: ChatRole::System,
                content: ASSESSMENT_PROMPT.to_string(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: user_message,
            },
        ];

        let options = LlmCallOptions {
            prompt_id: "goggles_review.assess",
            thread_id: None,
            expected_format: LlmExpectedFormat::JsonObject,
            max_output_tokens: Some(2000),
            reasoning_effort: Some(ReasoningEffort::Medium),
            temperature: Some(0.1),
            top_p: None,
            timeout_secs: Some(60),
            model: None,
        };

        let response = ctx
            .llm_chat(&messages, &options)
            .await
            .map_err(|e| format!("LLM assessment failed: {}", e))?;

        let parsed: Value = serde_json::from_str(&response).unwrap_or_else(|_| {
            serde_json::json!({
                "verdict": "needs_human_review",
                "confidence": 0.0,
                "reasoning": format!("Failed to parse LLM response as JSON. Raw: {}", &response[..response.len().min(500)]),
                "citations": []
            })
        });

        Ok(parsed)
    }
}
