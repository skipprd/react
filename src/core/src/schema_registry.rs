use once_cell::sync::OnceCell;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable identifiers for JSON Schemas enforced at runtime.
///
/// These IDs are the single source of truth across:
/// - prompt contracts (what the LLM must emit)
/// - provider adapters (transport-level schema when available)
/// - runtime parsing/validation (fallback when transport can't enforce)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SchemaId {
    AgentStepV1,
}

impl SchemaId {
    pub fn name(&self) -> &'static str {
        match self {
            SchemaId::AgentStepV1 => "agent.step.v1",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteEnvelopeV1 {
    pub kind: String,
    /// JSON-encoded payload.
    ///
    /// OpenAI Structured Outputs cannot represent arbitrary JSON objects (`additionalProperties`
    /// must be false on objects), so we transport arbitrary payloads as a JSON string and parse it
    /// at runtime.
    pub payload: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStepTypeV1 {
    Tool,
    Complete,
}

/// Exactly one agent step result (wire format).
///
/// This is intentionally a single object schema (no `oneOf`), because OpenAI Structured Outputs
/// rejects schemas containing `oneOf` (and disallows open-ended objects).
///
/// We emulate optional fields by making them nullable and requiring all fields (OpenAI constraint).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentStepV1 {
    #[serde(rename = "type")]
    pub type_: AgentStepTypeV1,

    /// Tool name when `type` is `"tool"`, otherwise null.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// JSON-encoded args when `type` is `"tool"`, otherwise null.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,

    /// Complete envelope when `type` is `"complete"`, otherwise null.
    #[serde(rename = "complete", default, skip_serializing_if = "Option::is_none")]
    pub complete: Option<CompleteEnvelopeV1>,
}

fn schema_for_id(id: SchemaId) -> Result<Value, crate::CoreError> {
    let schema = match id {
        SchemaId::AgentStepV1 => schemars::schema_for!(AgentStepV1),
    };
    let root_v = serde_json::to_value(&schema).map_err(|e| {
        crate::CoreError::Schema(format!("schema serialization failed for {id:?}: {e}"))
    })?;
    Ok(root_schema_json_to_json_schema_value(root_v))
}

pub fn strict_json_schema_for<T: JsonSchema>() -> Result<Value, crate::CoreError> {
    let root_v = serde_json::to_value(schemars::schema_for!(T))
        .map_err(|e| crate::CoreError::Schema(format!("schema serialization failed: {e}")))?;
    Ok(root_schema_json_to_json_schema_value(root_v))
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpenAiStrictSchema {
    name: String,
    schema: Value,
}

impl OpenAiStrictSchema {
    pub fn for_schema_id(id: SchemaId) -> Self {
        Self {
            name: id.name().to_string(),
            schema: json_schema(id),
        }
    }

    pub fn for_type<T: JsonSchema>(name: impl Into<String>) -> Result<Self, crate::CoreError> {
        Ok(Self {
            name: name.into(),
            schema: strict_json_schema_for::<T>()?,
        })
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn schema(&self) -> &Value {
        &self.schema
    }
}

/// Convert `schemars::schema_for!()` JSON into a standard JSON Schema document.
///
/// `schemars::schema_for!()` yields a wrapper like:
/// `{ "$schema": "...", "schema": { ...actual schema... }, "definitions": { ... } }`
///
/// OpenAI expects `text.format.schema` to be the schema itself (top-level `type:"object"`),
/// not wrapped under `schema`.
fn root_schema_json_to_json_schema_value(root_v: Value) -> Value {
    let mut out = if let Some(schema_v) = root_v.get("schema") {
        let mut inner = schema_v.clone();
        for defs_key in ["definitions", "$defs"] {
            if let Some(defs) = root_v.get(defs_key) {
                if defs.as_object().is_some_and(|m| !m.is_empty()) {
                    if let Some(obj) = inner.as_object_mut() {
                        obj.insert(defs_key.to_string(), defs.clone());
                    }
                }
            }
        }
        if let Some(meta) = root_v.get("$schema") {
            if let Some(obj) = inner.as_object_mut() {
                obj.insert("$schema".to_string(), meta.clone());
            }
        }
        inner
    } else {
        root_v
    };

    if let Some(obj) = out.as_object_mut() {
        obj.entry("type".to_string())
            .or_insert_with(|| Value::String("object".to_string()));
    }
    openai_structured_outputs_strictify_schema(&mut out);
    out
}

/// OpenAI Structured Outputs has additional JSON Schema constraints beyond Draft 2020-12.
///
/// In particular, for any object schema with `properties`, OpenAI requires:
/// - `required` MUST be present
/// - `required` MUST include *every* key present in `properties`
/// - (Practically) `additionalProperties: false` is expected for strict schemas
fn openai_structured_outputs_strictify_schema(v: &mut Value) {
    match v {
        Value::Array(a) => {
            for x in a.iter_mut() {
                openai_structured_outputs_strictify_schema(x);
            }
        }
        Value::Object(m) => {
            if let Some(ref_value) = m.get("$ref").cloned() {
                m.clear();
                m.insert("$ref".to_string(), ref_value);
                return;
            }
            for v in m.values_mut() {
                openai_structured_outputs_strictify_schema(v);
            }
            ensure_all_properties_required(m);
        }
        _ => {}
    }
}

fn ensure_all_properties_required(m: &mut serde_json::Map<String, Value>) {
    let prop_keys: Vec<String> = match m.get("properties").and_then(|p| p.as_object()) {
        Some(props) => props.keys().cloned().collect(),
        None => return,
    };

    m.entry("additionalProperties".to_string())
        .or_insert(Value::Bool(false));

    let req = m
        .entry("required".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));

    let mut req_set: Vec<String> = req
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    for k in &prop_keys {
        if !req_set.contains(k) {
            req_set.push(k.clone());
        }
    }
    req_set.sort();
    req_set.dedup();
    *req = Value::Array(req_set.into_iter().map(Value::String).collect());
}

static SCHEMAS: OnceCell<std::collections::HashMap<SchemaId, Value>> = OnceCell::new();
pub fn json_schema(id: SchemaId) -> Value {
    let map = SCHEMAS.get_or_init(|| {
        use std::collections::HashMap;
        let mut m: HashMap<SchemaId, Value> = HashMap::new();
        m.insert(
            SchemaId::AgentStepV1,
            schema_for_id(SchemaId::AgentStepV1).expect("AgentStepV1 schema init"),
        );
        m
    });
    map.get(&id)
        .cloned()
        .unwrap_or_else(|| schema_for_id(id).expect("schema_for_id fallback"))
}

pub fn validate(id: SchemaId, instance: &Value) -> Result<(), crate::CoreError> {
    let schema_json = json_schema(id);
    let validator = jsonschema::validator_for(&schema_json)
        .map_err(|e| crate::CoreError::Schema(format!("failed to compile {}: {}", id.name(), e)))?;
    if let Err(first) = validator.validate(instance) {
        return Err(crate::CoreError::Schema(format!(
            "{} validation error: {}",
            id.name(),
            first
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    enum LocalTier {
        Silver,
        Gold,
        Unknown,
    }

    impl Default for LocalTier {
        fn default() -> Self {
            Self::Unknown
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct LocalSchemaOutput {
        #[serde(default)]
        tier: LocalTier,
    }

    fn collect_ref_sibling_violations(v: &Value, path: &str, out: &mut Vec<String>) {
        match v {
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    collect_ref_sibling_violations(item, &format!("{path}[{i}]"), out);
                }
            }
            Value::Object(map) => {
                if map.contains_key("$ref") && map.len() > 1 {
                    let mut keys: Vec<String> = map.keys().cloned().collect();
                    keys.sort();
                    out.push(format!("{path}: {keys:?}"));
                }
                for (k, child) in map {
                    collect_ref_sibling_violations(child, &format!("{path}.{k}"), out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn agent_step_schema_accepts_tool_and_complete() {
        let tool = serde_json::json!({
            "type": "tool",
            "name": "file",
            "args": "{\"op\":\"list\",\"prefix\":\"models/\"}",
            "complete": null
        });
        validate(SchemaId::AgentStepV1, &tool).expect("tool should validate");

        let comp = serde_json::json!({
            "type": "complete",
            "name": null,
            "args": null,
            "complete": { "kind": "generic", "payload": "{\"text\":\"ok\"}", "display": "ok" }
        });
        validate(SchemaId::AgentStepV1, &comp).expect("complete should validate");
    }

    #[test]
    fn agent_step_schema_rejects_legacy_action_envelope() {
        let legacy = serde_json::json!({ "action": "file", "args": { "op": "list" } });
        let err = validate(SchemaId::AgentStepV1, &legacy).expect_err("should reject legacy");
        assert!(err.to_string().contains("validation error"), "err={err}");
    }

    #[test]
    fn schema_documents_have_object_type_at_top_level() {
        let s = json_schema(SchemaId::AgentStepV1);
        assert_eq!(
            s.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "schema {} must be a top-level object schema for OpenAI; schema={}",
            SchemaId::AgentStepV1.name(),
            s
        );
    }

    #[test]
    fn complete_envelope_required_includes_display_for_openai() {
        let s = json_schema(SchemaId::AgentStepV1);
        let defs = s.get("$defs").and_then(|v| v.as_object()).expect("$defs");
        let env = defs.get("CompleteEnvelopeV1").expect("CompleteEnvelopeV1");
        let req = env
            .get("required")
            .and_then(|v| v.as_array())
            .expect("required");
        let mut got: Vec<String> = req
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        got.sort();
        assert!(
            got.contains(&"display".to_string()),
            "expected display required (nullable) for OpenAI strict schema; got={got:?}"
        );
    }

    #[test]
    fn agent_step_schema_has_no_ref_siblings_for_openai() {
        let s = json_schema(SchemaId::AgentStepV1);
        let mut violations: Vec<String> = Vec::new();
        collect_ref_sibling_violations(&s, "$", &mut violations);
        assert!(
            violations.is_empty(),
            "OpenAI-incompatible $ref sibling nodes found:\n{}",
            violations.join("\n")
        );
    }

    #[test]
    fn openai_strict_schema_for_type_strips_ref_sibling_defaults() {
        let schema =
            OpenAiStrictSchema::for_type::<LocalSchemaOutput>("core.tests.local_schema_output")
                .expect("schema");
        let mut violations: Vec<String> = Vec::new();
        collect_ref_sibling_violations(schema.schema(), "$", &mut violations);
        assert!(
            violations.is_empty(),
            "OpenAI-incompatible $ref sibling nodes found:\n{}",
            violations.join("\n")
        );
    }
}
