use async_trait::async_trait;
use serde_json::Value;

use react_core::agent::AgentCtx;
use react_core::session::ThreadStore;
use react_core::tools::Tool;

pub struct ListThreadsTool;

#[async_trait]
impl Tool for ListThreadsTool {
    fn name(&self) -> &'static str {
        "list_threads"
    }

    async fn call(&self, _args: Value, ctx: &AgentCtx) -> Result<Value, String> {
        let store = ThreadStore::new(
            ctx.storage().clone(),
            ctx.scope().clone(),
            ctx.keyspace().clone(),
        );
        let ids = store.list().await;
        Ok(serde_json::json!({ "thread_ids": ids, "count": ids.len() }))
    }
}
