# Suites

A **suite** is a product surface that defines how the agent behaves for a particular workflow. Each suite owns its tool registry, system prompts, agent policy, and optional preflight logic.

## What a suite provides

| Component | Purpose |
|---|---|
| **Tool registry** | Which tools the agent can call (SQL execution, dbt validation, vector search, etc.) |
| **System prompt** | Instructions and persona for the LLM |
| **Tool card** | Descriptions of each tool, injected into the prompt so the LLM knows what is available |
| **Agent policy** | Rules for validating finals, triggering interrupts, injecting prelude context |
| **Preflight** | Optional setup that runs before the agent loop (e.g. dataset discovery, catalog refresh) |
| **Phase order** | Optional ordered list of phases for multi-step workflows |

## Registered suites

The default suite registry (`src/suites/react-suites/src/registry.rs`) includes:

| Suite ID | Label | Agent types | Description |
|---|---|---|---|
| `data_engineer` | Data Engineer | `ask`, `agent`, `review` | Analytics + dbt workflow with phased execution (preflight, plan, author, validate, review, publish) |
| `kb` | KB | `kb` | Knowledge-base workflow for ingesting documents and answering questions via vector search |

## The Suite trait

Every suite implements the `Suite` trait:

```rust
#[async_trait]
pub trait Suite: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn supported_agent_types(&self) -> Vec<String>;
    fn default_agent_type(&self) -> &'static str;
    fn phase_order(&self, agent_type: &str) -> Vec<String>;

    async fn handle_new(...) -> Result<Vec<FlowFrame>, String>;
    async fn handle_open(...) -> Result<Vec<FlowFrame>, String>;
    async fn handle_user(...) -> Result<Vec<FlowFrame>, String>;
}
```

The three `handle_*` methods correspond to the client frame types `new`, `open`, and `user`. Each returns a list of `FlowFrame`s that the WebSocket server translates into response frames.

## Suite context

Suites receive a `SuiteCtx` containing all injected capabilities:

- `QueryProvider` — SQL execution and schema discovery
- `DatasetCatalogProvider` — dataset discovery
- `CatalogProvider` — semantic catalog metadata
- `VectorStore` — embeddings upsert and query
- `DbtProvider` — dbt project scaffolding, validation, and publishing
- `StorageAdapter` — persistence (local or S3)
- `Keyspace` — scoped key builder for persistence paths
- `RequestScope` — tenant, workspace, project_id

The suite selects which providers to wire into the `AgentCtx` for the loop.

## Suite selection

Clients specify `suiteId` in their `new` or `open` frame. The WebSocket server looks up the suite in the registry. If the suite is not found, an `error` frame is returned.

```json
{
  "v": 1,
  "type": "new",
  "cid": "...",
  "suiteId": "data_engineer",
  "agentType": "agent",
  "question": "Build a staging model for the orders table"
}
```

## Agent types

Each suite defines which agent types (modes) it supports. For example, the Data Engineer suite supports `ask` (question answering), `agent` (full workflow), and `review` (red-team review). The KB suite supports `kb` (knowledge-base Q&A).

The `agentType` field in the client frame selects the mode. Different agent types within the same suite may use different tool registries, prompts, and policies.

## Next steps

- [Data Engineer Suite](../suites/data-engineer.md) — the default analytics + dbt workflow
- [Knowledge Base Suite](../suites/kb.md) — document ingestion and semantic search
- [Extending: Custom Suite](../extending/custom-suite.md) — building your own suite
