use react_core::agent::AgentCtx;
use serde_json::Value;
use std::collections::HashSet;

use crate::progress_controller::{BatchFailureKind, DataEngineerEvent};
use crate::state_manager;

pub(crate) fn extract_batch_failure_kind(res: &Value) -> Result<BatchFailureKind, String> {
    let raw = res
        .get("batch_failure_kind")
        .cloned()
        .ok_or_else(|| "missing required field batch_failure_kind".to_string())?;
    serde_json::from_value::<BatchFailureKind>(raw)
        .map_err(|e| format!("invalid batch_failure_kind value: {e}"))
}

pub(crate) fn classify_schema_batch_failure_kind(msg: &str) -> BatchFailureKind {
    let s = msg.to_ascii_lowercase();
    if s.contains("timeout")
        || s.contains("temporar")
        || s.contains("http 502")
        || s.contains("http 503")
        || s.contains("http 504")
    {
        return BatchFailureKind::InfraTransient;
    }
    if s.contains("schema") || s.contains("yaml") || s.contains("parse") || s.contains("column") {
        return BatchFailureKind::SchemaOrContract;
    }
    BatchFailureKind::Unknown
}

pub(crate) fn classify_authoring_batch_failure_kind(msg: &str) -> BatchFailureKind {
    let s = msg.to_ascii_lowercase();
    if s.contains("timeout")
        || s.contains("temporar")
        || s.contains("http 502")
        || s.contains("http 503")
        || s.contains("http 504")
    {
        return BatchFailureKind::InfraTransient;
    }
    if s.contains("sql validation")
        || s.contains("athena/trino")
        || s.contains("trino")
        || s.contains("materialized sql")
        || s.contains("source() call")
        || s.contains("dbt source()")
        || s.contains("syntax error")
    {
        return BatchFailureKind::SqlValidation;
    }
    if s.contains("schema")
        || s.contains("yaml")
        || s.contains("parse")
        || s.contains("contract")
        || s.contains("invalid model folder")
    {
        return BatchFailureKind::SchemaOrContract;
    }
    BatchFailureKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_authoring_sql_validation() {
        let k = classify_authoring_batch_failure_kind(
            "sql validation failed: Athena/Trino cannot reference a SELECT-list alias",
        );
        assert_eq!(k, BatchFailureKind::SqlValidation);
    }

    #[test]
    fn classify_authoring_schema_error() {
        let k = classify_authoring_batch_failure_kind("invalid model folder 'models/raw/x.sql'");
        assert_eq!(k, BatchFailureKind::SchemaOrContract);
    }
}

pub(crate) async fn emit_batch_event(ctx: &AgentCtx, event: DataEngineerEvent) -> Result<(), String> {
    let Some(thread_store) = ctx.thread_store().as_ref() else {
        return Ok(());
    };
    let Some(thread_id) = ctx.thread_id().as_deref() else {
        return Ok(());
    };
    state_manager::apply_execution_event(&thread_store.control_store(), thread_id, event)
        .await
        .map(|_| ())
}

pub(crate) fn parse_succeeded_ids(res: &Value, field: &str) -> Vec<String> {
    res.get(field)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn derive_failed_ids(attempted: &[String], succeeded: &[String]) -> Vec<String> {
    let succ_set: HashSet<String> = succeeded.iter().cloned().collect();
    attempted
        .iter()
        .filter(|id| !succ_set.contains(*id))
        .cloned()
        .collect()
}

pub(crate) fn extract_first_error(res: &Value, default: &str) -> String {
    res.get("errors")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default.to_string())
}
