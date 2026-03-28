use async_trait::async_trait;
use serde_json::Value;

use react_core::agent::AgentCtx;
use react_core::session::analysis;
use react_core::session::ThreadStore;
use react_core::tools::Tool;

pub struct SummarizeThreadTool;

#[async_trait]
impl Tool for SummarizeThreadTool {
    fn name(&self) -> &'static str {
        "summarize_thread"
    }

    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String> {
        let thread_id = args
            .get("thread_id")
            .and_then(|v| v.as_str())
            .ok_or("thread_id is required")?;

        let store = ThreadStore::new(
            ctx.storage().clone(),
            ctx.scope().clone(),
            ctx.keyspace().clone(),
        );

        let log = store
            .get(thread_id)
            .await
            .map_err(|e| format!("failed to read thread: {e}"))?;

        let summary = analysis::summarize(&log);
        serde_json::to_value(&summary).map_err(|e| format!("serialization error: {e}"))
    }
}
