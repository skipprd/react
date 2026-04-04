use async_trait::async_trait;
use serde_json::Value;

use react_core::agent::AgentCtx;
use react_core::provider_traits::query_typed_documents;
use react_core::tools::Tool;

pub struct KbSearchTool;

#[async_trait]
impl Tool for KbSearchTool {
    fn name(&self) -> &'static str {
        "kb_search"
    }

    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String> {
        let query = args
            .get("query")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let k = args.get("k").and_then(|x| x.as_u64()).unwrap_or(8) as usize;
        let dataset_id = args
            .get("dataset_id")
            .and_then(|x| x.as_str())
            .unwrap_or("kb");

        if query.trim().is_empty() {
            return Err("query is required".to_string());
        }

        let vector = ctx
            .vector()
            .as_ref()
            .ok_or_else(|| "vector provider missing".to_string())?;
        let mut vecs = ctx
            .llm_embed(&[query.clone()])
            .map_err(|e| format!("embed failed: {}", e))?;
        let qv = vecs.pop().unwrap_or_default();
        if qv.is_empty() {
            return Err("empty embedding vector".to_string());
        }

        let mut hits = query_typed_documents::<crate::vector_docs::KbDocCollection>(
            vector.as_ref(),
            ctx.scope(),
            &qv,
            k * 5,
        )
        .await?;
        hits.retain(|h| h.item.metadata().dataset_id == dataset_id);
        hits.sort_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(k);

        let items: Vec<Value> = hits
            .into_iter()
            .map(|h| {
                serde_json::json!({
                    "dataset_id": h.item.metadata().dataset_id,
                    "text": h.item.text(),
                    "score": h.score,
                    "meta": {
                        "path": h.item.metadata().path,
                        "chunk_index": h.item.metadata().chunk_index
                    }
                })
            })
            .collect();

        Ok(serde_json::json!({"ok": true, "items": items}))
    }
}
