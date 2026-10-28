use serde_json::Value;

use super::{Agent, AgentStep, AgentStepTypeV1, AgentStepV1, CompleteEnvelope, SchemaId};
use crate::error::CoreError;
use crate::json_repair;
use crate::parse_error::StepParseError;
use crate::schema_registry::CompleteEnvelopeV1;

impl TryFrom<CompleteEnvelopeV1> for CompleteEnvelope {
    type Error = StepParseError;

    fn try_from(comp: CompleteEnvelopeV1) -> Result<Self, Self::Error> {
        let payload: Value = json_repair::resilient_parse_string_field(&comp.payload)
            .unwrap_or_else(|_| Value::String(comp.payload.clone()));
        Ok(Self {
            kind: comp.kind,
            payload,
            display: comp.display,
        })
    }
}

impl TryFrom<AgentStepV1> for AgentStep {
    type Error = StepParseError;

    fn try_from(step: AgentStepV1) -> Result<Self, Self::Error> {
        match (step.type_, step.name, step.args, step.complete) {
            (AgentStepTypeV1::Tool, Some(name), Some(args_json), None) => {
                let args: Value = json_repair::resilient_parse_string_field(&args_json)
                    .map_err(StepParseError::Args)?;
                Ok(AgentStep::Tool { name, args })
            }
            (AgentStepTypeV1::Complete, None, None, Some(comp)) => Ok(AgentStep::Complete {
                complete_env: CompleteEnvelope::try_from(comp)?,
            }),
            (t, n, a, c) => Err(StepParseError::ShapeMismatch(format!(
                "type={t:?} name={} args={} complete={}",
                n.is_some(),
                a.is_some(),
                c.is_some()
            ))),
        }
    }
}

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

    pub(crate) fn parse_agent_step(raw: &str) -> Result<AgentStep, CoreError> {
        let cleaned = Self::strip_markdown_code_fences(raw);
        let trimmed = cleaned.trim();

        let v: Value = json_repair::resilient_parse(trimmed).map_err(StepParseError::NotJson)?;

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

        crate::schema_registry::validate(SchemaId::AgentStepV1, &v)
            .map_err(|e| StepParseError::Schema(e.to_string()))?;
        let step: AgentStepV1 = serde_json::from_value::<AgentStepV1>(v).map_err(|e| {
            StepParseError::ShapeMismatch(format!(
                "failed to deserialize {}: {e}",
                SchemaId::AgentStepV1.name()
            ))
        })?;

        AgentStep::try_from(step).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_with_name_is_shape_mismatch() {
        let raw = "{\"type\":\"complete\",\"name\":\"oops\",\"args\":null,\"complete\":{\"kind\":\"generic\",\"payload\":\"{}\",\"display\":null}}";
        let err = Agent::parse_agent_step(raw).expect_err("must not strip name");
        match err {
            CoreError::Parse(StepParseError::ShapeMismatch(msg)) => {
                assert!(msg.contains("name=true"), "{msg}");
            }
            other => panic!("expected shape mismatch, got {other:?}"),
        }
    }

    #[test]
    fn complete_with_args_is_shape_mismatch() {
        let raw = "{\"type\":\"complete\",\"name\":null,\"args\":\"{}\",\"complete\":{\"kind\":\"generic\",\"payload\":\"{}\",\"display\":null}}";
        let err = Agent::parse_agent_step(raw).expect_err("must not strip args");
        match err {
            CoreError::Parse(StepParseError::ShapeMismatch(msg)) => {
                assert!(msg.contains("args=true"), "{msg}");
            }
            other => panic!("expected shape mismatch, got {other:?}"),
        }
    }
}
