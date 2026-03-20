# Custom Tool

Tools are the actions the agent can invoke during the ReAct loop. Implement the `Tool` trait to create new capabilities.

## The Tool trait

```rust
use async_trait::async_trait;
use serde_json::Value;
use react_core::agent::AgentCtx;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String>;
}
```

- `name()` — returns the tool name as the agent sees it. Must be unique within the tool registry.
- `call()` — receives the JSON `args` from the LLM's tool action and returns a JSON observation (or an error string).

## Example

A tool that runs a SQL query:

```rust
pub struct SqlRunTool;

#[async_trait]
impl Tool for SqlRunTool {
    fn name(&self) -> &'static str {
        "sql_run"
    }

    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String> {
        let sql = args.get("sql")
            .and_then(|v| v.as_str())
            .ok_or("missing 'sql' argument")?;

        let query_provider = ctx.query.as_ref()
            .ok_or("no query provider configured")?;

        let result = query_provider.execute(sql).await?;

        Ok(serde_json::to_value(&result).map_err(|e| e.to_string())?)
    }
}
```

## Accessing providers

The `AgentCtx` gives tools access to all injected providers:

| Field | Type | Description |
|---|---|---|
| `ctx.query` | `Option<Arc<dyn QueryProvider>>` | SQL execution and schema |
| `ctx.warehouse` | `Arc<dyn WarehouseProvider>` | Warehouse naming/metadata |
| `ctx.dbt` | `Option<Arc<dyn DbtProvider>>` | dbt operations |
| `ctx.vector` | `Option<Arc<dyn VectorStore>>` | Vector embeddings |
| `ctx.storage` | `Arc<dyn StorageAdapter>` | Key-value persistence |
| `ctx.keyspace` | `Arc<dyn Keyspace>` | Scoped key builder |
| `ctx.scope` | `RequestScope` | Tenant, workspace, project_id |
| `ctx.llm` | `Arc<dyn LargeLanguageModel>` | LLM for sub-calls |
| `ctx.thread_store` | `Option<ThreadStore>` | Thread persistence |
| `ctx.resolved_config` | `Option<Arc<ReactResolvedConfig>>` | Resolved runtime config |

## Registering tools

Register tools in the suite's tool registry builder:

```rust
fn build_tools(&self, sctx: &SuiteCtx) -> Result<ToolRegistry, String> {
    let mut reg = ToolRegistry::new();
    reg.register(SqlRunTool);
    reg.register(MyNewTool);
    Ok(reg)
}
```

## Tool card

The agent needs to know what tools are available. The system prompt includes a tool card — a text description of each tool's name, purpose, and expected arguments. This is built by the suite and injected into the prompt.

When adding a new tool, update the suite's prompt module to include the tool in the tool card.

## Error handling

- Return `Err(String)` for fatal errors that should stop the tool
- Return `Ok(json!({"error": "..."}))` for recoverable errors that the LLM should see and react to
- The agent loop captures tool errors as observations and feeds them back so the LLM can self-correct

## Next steps

- [Custom Suite](custom-suite.md) — building the suite that uses tools
- [Custom Provider](custom-provider.md) — implementing new provider backends
