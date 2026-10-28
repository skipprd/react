use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use crate::agent::AgentCtx;
use crate::session::ToolObservation;
use crate::text::truncate_utf8_bytes;

pub const MAX_OUTPUT_BYTES: usize = 16 * 1024;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str {
        ""
    }
    fn max_output_bytes(&self) -> usize {
        MAX_OUTPUT_BYTES
    }
    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String>;
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
#[error("duplicate tool registered: {name}")]
pub struct DuplicateTool {
    pub name: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError {
    pub message: String,
}

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolOutcome {
    Ok(Value),
    Failed(ToolError),
}

impl ToolOutcome {
    pub fn from_result(r: Result<Value, String>, max_bytes: usize) -> Self {
        match r {
            Ok(v) => Self::Ok(bound_tool_value(v, max_bytes)),
            Err(e) => Self::Failed(ToolError::new(e)),
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    pub fn value(&self) -> Option<&Value> {
        match self {
            Self::Ok(v) => Some(v),
            Self::Failed(_) => None,
        }
    }

    pub fn as_interrupt_value(&self) -> Value {
        match self {
            Self::Ok(v) => v.clone(),
            Self::Failed(e) => serde_json::json!({"errors": [e.message]}),
        }
    }

    pub fn observation(&self) -> ToolObservation {
        match self {
            Self::Ok(v) => ToolObservation::from_success(v),
            Self::Failed(e) => ToolObservation::from_failure(e.message.clone()),
        }
    }
}

pub fn bound_tool_value(v: Value, max_bytes: usize) -> Value {
    let Ok(s) = serde_json::to_string(&v) else {
        return v;
    };
    if s.len() <= max_bytes {
        return v;
    }
    serde_json::json!({
        "truncated": true,
        "bytes": s.len(),
        "preview": truncate_utf8_bytes(&s, max_bytes.saturating_sub(80)),
    })
}

pub struct ToolRegistry {
    tools: HashMap<&'static str, Box<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }
    pub fn register<T: Tool + 'static>(&mut self, t: T) -> Result<(), DuplicateTool> {
        let name = t.name();
        if self.tools.contains_key(name) {
            return Err(DuplicateTool { name });
        }
        self.tools.insert(name, Box::new(t));
        Ok(())
    }

    pub async fn call(&self, name: &str, args: Value, ctx: &AgentCtx) -> ToolOutcome {
        match self.tools.get(name) {
            Some(t) => {
                let max = t.max_output_bytes();
                ToolOutcome::from_result(t.call(args, ctx).await, max)
            }
            None => ToolOutcome::Failed(ToolError::new(format!("unknown tool '{name}'"))),
        }
    }

    pub fn tool_card(&self) -> String {
        let mut names: Vec<_> = self.tools.iter().collect();
        names.sort_by_key(|(n, _)| *n);
        let mut s = String::from("Available tools:");
        for (name, t) in names {
            let d = t.description();
            if d.is_empty() {
                s.push_str(&format!("\n- {name}"));
            } else {
                s.push_str(&format!("\n- {name}: {d}"));
            }
        }
        s
    }

    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().copied().collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentCtxBuilder, DefaultPolicy};
    use crate::keyspace::DefaultKeyspace;
    use crate::llm::NullModel;
    use crate::scope::RequestScope;
    use crate::test_support::InMemoryStorageAdapter;
    use std::sync::Arc;

    struct RowsTool;
    #[async_trait]
    impl Tool for RowsTool {
        fn name(&self) -> &'static str {
            "rows"
        }
        fn description(&self) -> &'static str {
            "returns rows"
        }
        async fn call(&self, _args: Value, _ctx: &AgentCtx) -> Result<Value, String> {
            Ok(serde_json::json!({"rows": [1, 2, 3]}))
        }
    }

    struct DupTool;
    #[async_trait]
    impl Tool for DupTool {
        fn name(&self) -> &'static str {
            "rows"
        }
        async fn call(&self, _args: Value, _ctx: &AgentCtx) -> Result<Value, String> {
            Ok(serde_json::json!({}))
        }
    }

    struct BigTool;
    #[async_trait]
    impl Tool for BigTool {
        fn name(&self) -> &'static str {
            "big"
        }
        fn max_output_bytes(&self) -> usize {
            64
        }
        async fn call(&self, _args: Value, _ctx: &AgentCtx) -> Result<Value, String> {
            Ok(serde_json::json!({"blob": "x".repeat(400)}))
        }
    }

    fn test_ctx() -> AgentCtx {
        AgentCtxBuilder::new(
            Arc::new(NullModel {}),
            Arc::new(InMemoryStorageAdapter::default()),
            RequestScope::parse("t", "w", "p").expect("valid test scope"),
            Arc::new(DefaultKeyspace::new("bucket".to_string())),
            Arc::new(DefaultPolicy),
        )
        .build()
    }

    #[tokio::test]
    async fn ok_value_without_ok_key_is_success() {
        let mut reg = ToolRegistry::new();
        reg.register(RowsTool).expect("unique");
        let out = reg.call("rows", serde_json::json!({}), &test_ctx()).await;
        assert!(out.is_ok(), "missing ok key must not fail: {out:?}");
        assert_eq!(out.value().and_then(|v| v.get("rows")), Some(&json_arr()));
    }

    fn json_arr() -> Value {
        serde_json::json!([1, 2, 3])
    }

    #[test]
    fn duplicate_register_is_err() {
        let mut reg = ToolRegistry::new();
        reg.register(RowsTool).expect("unique");
        let err = reg.register(DupTool).expect_err("duplicate");
        assert_eq!(err.name, "rows");
    }

    #[test]
    fn tool_card_is_generated_from_registry() {
        let mut reg = ToolRegistry::new();
        reg.register(RowsTool).expect("unique");
        let card = reg.tool_card();
        assert!(card.contains("- rows: returns rows"));
    }

    #[test]
    fn empty_registry_tool_card_is_available_header_only() {
        let card = ToolRegistry::new().tool_card();
        assert_eq!(card, "Available tools:");
    }

    #[tokio::test]
    async fn oversized_output_is_truncated() {
        let mut reg = ToolRegistry::new();
        reg.register(BigTool).expect("unique");
        let out = reg.call("big", serde_json::json!({}), &test_ctx()).await;
        let v = out.value().expect("ok");
        assert_eq!(v.get("truncated").and_then(|x| x.as_bool()), Some(true));
    }
}
