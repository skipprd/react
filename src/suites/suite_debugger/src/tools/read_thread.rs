use async_trait::async_trait;
use serde_json::Value;

use react_core::agent::AgentCtx;
use react_core::session::ThreadStore;
use react_core::tools::Tool;

pub struct ReadThreadTool;

#[async_trait]
impl Tool for ReadThreadTool {
    fn name(&self) -> &'static str {
        "read_thread"
    }

    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String> {
        let thread_id = args
            .get("thread_id")
            .and_then(|v| v.as_str())
            .ok_or("thread_id is required")?;
        let target_scope = crate::capabilities::target_scope_for_agent(ctx);

        let store = ThreadStore::new(ctx.storage().clone(), target_scope, ctx.keyspace().clone());

        let log = store
            .get(thread_id)
            .await
            .map_err(|e| format!("failed to read thread: {e}"))?;

        let step_count = log.steps.len();
        let steps: Vec<Value> = log
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mut v = serde_json::to_value(s).unwrap_or(Value::Null);
                if let Value::Object(ref mut m) = v {
                    m.insert("_index".into(), Value::Number(i.into()));
                }
                v
            })
            .collect();

        Ok(serde_json::json!({
            "thread_id": thread_id,
            "title": log.title,
            "step_count": step_count,
            "result": log.result,
            "steps": steps,
        }))
    }
}
