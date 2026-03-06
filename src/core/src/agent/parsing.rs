use serde_json::Value;

use super::llm_gateway::escape_control_chars_in_json_strings;
use super::{Agent, AgentStepTypeV1, AgentStepV1, CompleteEnvelope, ParsedStep, SchemaId};

impl Agent {
    fn strip_markdown_code_fences(raw: &str) -> String {
        let t = raw.trim();
        if !t.starts_with("```") {
            return t.to_string();
        }
        // Handle ```json ... ``` and ``` ... ```
        let t = t
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim();
        if let Some(end) = t.rfind("```") {
            return t[..end].trim().to_string();
        }
        t.to_string()
    }

    pub(crate) fn parse_agent_step(raw: &str) -> Result<ParsedStep, String> {
        let cleaned = Self::strip_markdown_code_fences(raw);
        let trimmed = cleaned.trim();

        fn parse_json_from_model_string_field(raw_json: &str, what: &str) -> Result<Value, String> {
            match serde_json::from_str::<Value>(raw_json) {
                Ok(v) => Ok(v),
                Err(e) => {
                    // Repair raw control chars inside JSON strings (literal newlines, etc).
                    let repaired = escape_control_chars_in_json_strings(raw_json);
                    serde_json::from_str::<Value>(&repaired)
                        .map_err(|_| format!("{what} is not valid JSON string: {e}"))
                }
            }
        }

        let v = match serde_json::from_str::<Value>(trimmed) {
            Ok(v) => v,
            Err(e) => {
                // Conservative repair: escape control chars inside strings (raw newlines, etc).
                let repaired = escape_control_chars_in_json_strings(trimmed);
                serde_json::from_str::<Value>(&repaired)
                    .map_err(|_| format!("invalid JSON from model: {}", e))?
            }
        };

        crate::schema_registry::validate(SchemaId::AgentStepV1, &v)?;
        let step: AgentStepV1 = serde_json::from_value::<AgentStepV1>(v).map_err(|e| {
            format!(
                "failed to deserialize {}: {}",
                SchemaId::AgentStepV1.name(),
                e
            )
        })?;

        match step.type_ {
            AgentStepTypeV1::Tool => {
                let Some(name) = step.name else {
                    return Err("agent.step.v1 validation error: missing tool name".to_string());
                };
                let Some(args_json) = step.args else {
                    return Err("agent.step.v1 validation error: missing tool args".to_string());
                };
                if step.complete.is_some() {
                    return Err(
                        "agent.step.v1 validation error: tool step must not include complete".to_string(),
                    );
                }
                let args: Value = parse_json_from_model_string_field(
                    &args_json,
                    "agent.step.v1 validation error: args",
                )?;
                Ok(ParsedStep::Tool { name, args })
            }
            AgentStepTypeV1::Complete => {
                if step.name.is_some() || step.args.is_some() {
                    return Err(
                        "agent.step.v1 validation error: complete step must not include name/args"
                            .to_string(),
                    );
                }
                let Some(comp) = step.complete else {
                    return Err("agent.step.v1 validation error: missing complete".to_string());
                };
                let payload: Value = parse_json_from_model_string_field(
                    &comp.payload,
                    "agent.step.v1 validation error: complete.payload",
                )?;
                Ok(ParsedStep::Complete {
                    complete_env: CompleteEnvelope {
                        kind: comp.kind,
                        payload,
                        display: comp.display,
                    },
                })
            }
        }
    }
}
