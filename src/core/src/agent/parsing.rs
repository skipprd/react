use serde_json::Value;

use super::{Agent, AgentStepTypeV1, AgentStepV1, CompleteEnvelope, ParsedStep, SchemaId};
use crate::error::CoreError;
use crate::json_repair;

impl Agent {
    fn strip_markdown_code_fences(raw: &str) -> String {
        let t = raw.trim();
        if !t.starts_with("```") {
            return t.to_string();
        }
        let t = t
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim();
        if let Some(end) = t.rfind("```") {
            return t[..end].trim().to_string();
        }
        t.to_string()
    }

    pub(crate) fn parse_agent_step(raw: &str) -> Result<ParsedStep, CoreError> {
        let cleaned = Self::strip_markdown_code_fences(raw);
        let trimmed = cleaned.trim();

        let v: Value = json_repair::resilient_parse(trimmed).map_err(|_| {
            CoreError::Agent(format!(
                "invalid JSON from model: {}",
                serde_json::from_str::<Value>(trimmed).unwrap_err()
            ))
        })?;

        if let Some(first_end) = json_repair::find_first_json_object_end(trimmed) {
            let tail = trimmed[first_end..].trim();
            if !tail.is_empty() {
                let extra_count = json_repair::extract_all_json_values(tail, 20).len();
                if extra_count > 0 {
                    tracing::warn!(
                        "agent step response contained {} extra JSON object(s) after the first; \
                         structured output enforcement may have failed for this call",
                        extra_count
                    );
                }
            }
        }

        crate::schema_registry::validate(SchemaId::AgentStepV1, &v)?;
        let step: AgentStepV1 = serde_json::from_value::<AgentStepV1>(v).map_err(|e| {
            CoreError::Agent(format!(
                "failed to deserialize {}: {}",
                SchemaId::AgentStepV1.name(),
                e
            ))
        })?;

        match step.type_ {
            AgentStepTypeV1::Tool => {
                let Some(name) = step.name else {
                    return Err(CoreError::Agent(
                        "agent.step.v1 validation error: missing tool name".to_string(),
                    ));
                };
                let Some(args_json) = step.args else {
                    return Err(CoreError::Agent(
                        "agent.step.v1 validation error: missing tool args".to_string(),
                    ));
                };
                if step.complete.is_some() {
                    return Err(CoreError::Agent(
                        "agent.step.v1 validation error: tool step must not include complete"
                            .to_string(),
                    ));
                }
                let args: Value =
                    json_repair::resilient_parse_string_field(&args_json).map_err(|e| {
                        CoreError::Agent(format!("agent.step.v1 validation error: args {e}"))
                    })?;
                Ok(ParsedStep::Tool { name, args })
            }
            AgentStepTypeV1::Complete => {
                if step.name.is_some() || step.args.is_some() {
                    tracing::warn!(
                        "agent.step.v1: complete step included name={:?} args={:?}; stripping",
                        step.name,
                        step.args.as_deref().map(|s| &s[..s.len().min(60)]),
                    );
                }
                let Some(comp) = step.complete else {
                    return Err(CoreError::Agent(
                        "agent.step.v1 validation error: missing complete".to_string(),
                    ));
                };
                let payload: Value = json_repair::resilient_parse_string_field(&comp.payload)
                    .unwrap_or_else(|_| {
                        tracing::debug!(
                            "complete.payload is not valid JSON; treating as plain text ({} bytes)",
                            comp.payload.len(),
                        );
                        Value::String(comp.payload.clone())
                    });
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
