use async_trait::async_trait;
use serde_json::Value;

use react_core::agent::AgentCtx;
use react_core::provider_traits::query_typed_documents;
use react_core::tools::Tool;

pub struct AdminRepoQueryTool;

#[async_trait]
impl Tool for AdminRepoQueryTool {
    fn name(&self) -> &'static str {
        "admin_repo_query"
    }

    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if query.is_empty() {
            return Err("query is required".to_string());
        }
        let k = args.get("k").and_then(|v| v.as_u64()).unwrap_or(8) as usize;
        let vector = ctx
            .vector()
            .as_ref()
            .ok_or_else(|| "vector provider missing".to_string())?;
        let mut qv = ctx
            .llm_embed(&[query])
            .map_err(|e| format!("embed failed: {e}"))?;
        let qv = qv.pop().unwrap_or_default();
        if qv.is_empty() {
            return Err("empty embedding vector".to_string());
        }

        let hits = query_typed_documents::<crate::repo_index::AdminRepoCollection>(
            vector.as_ref(),
            ctx.scope(),
            &qv,
            k,
        )
        .await?;

        let items: Vec<Value> = hits
            .into_iter()
            .map(|hit| {
                serde_json::json!({
                    "repo_root": hit.item.metadata().repo_root,
                    "repo_label": hit.item.metadata().repo_label,
                    "path": hit.item.metadata().path,
                    "chunk_index": hit.item.metadata().chunk_index,
                    "sha256": hit.item.metadata().sha256,
                    "text": hit.item.text(),
                    "score": hit.score
                })
            })
            .collect();

        Ok(serde_json::json!({
            "ok": true,
            "items": items,
            "count": items.len()
        }))
    }
}
