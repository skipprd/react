use async_trait::async_trait;
use serde_json::Value;

use react_core::agent::AgentCtx;
use react_core::tools::Tool;

pub struct ReadRunLogTool;

#[async_trait]
impl Tool for ReadRunLogTool {
    fn name(&self) -> &'static str {
        "read_run_log"
    }

    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String> {
        let thread_id = args
            .get("thread_id")
            .and_then(|v| v.as_str())
            .ok_or("thread_id is required")?;
        let target_scope = crate::capabilities::target_scope_for_agent(ctx);

        let key = ctx
            .keyspace()
            .thread_log_key(&target_scope, thread_id)
            .map_err(|e| format!("invalid thread_id: {e}"))?;

        let bytes = ctx
            .storage()
            .get_bytes(&key)
            .await
            .map_err(|e| format!("failed to read run log: {e}"))?;

        let text = String::from_utf8_lossy(&bytes);

        let max_len = args
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(50_000) as usize;
        let truncated = text.len() > max_len;
        let output = if truncated {
            &text[..max_len]
        } else {
            &text
        };

        Ok(serde_json::json!({
            "thread_id": thread_id,
            "log": output,
            "truncated": truncated,
            "total_bytes": bytes.len(),
        }))
    }
}
