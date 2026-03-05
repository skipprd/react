# Knowledge Base Suite

The Knowledge Base suite (`kb`) provides document ingestion and semantic question-answering. It ingests local directories into a vector store and answers questions using retrieval-augmented generation.

## Suite ID and agent types

| Property | Value |
|---|---|
| Suite ID | `kb` |
| Agent types | `kb` |
| Default agent type | `kb` |

## Tools

| Tool | Purpose |
|---|---|
| `kb_ingest_dir` | Ingest a local directory into the vector store. Reads files, chunks content, generates embeddings, and upserts into LanceDB. |
| `kb_search` | Semantic search over ingested content. Returns the top-k most relevant chunks for a query. |

## How it works

### Ingestion

1. The user provides a directory path (e.g. "Ingest the docs in `./knowledge-base/`")
2. The `kb_ingest_dir` tool reads all files in the directory
3. Content is chunked and embedded using the configured embed model
4. Chunks are upserted into the LanceDB vector store scoped to the current keyspace

### Question answering

1. The user asks a question
2. The agent uses `kb_search` to find relevant chunks
3. The LLM synthesises an answer from the retrieved context
4. A `final` frame with kind `kb` is returned containing the answer

## Configuration

The KB suite requires:

- `providers.vector.enabled: true` — enables the LanceDB vector store
- `llm.embed_model` — embedding model for vector operations (e.g. `text-embedding-3-small`)

No warehouse or dbt provider is needed for KB-only workflows.

Minimal config:

```yaml
version: 1

storage:
  mode: local
  path: ./.react

scope:
  tenant: local
  workspace: dev
  project_id: kb_demo

llm:
  provider: OPENAI_COMPAT
  base_url: https://api.openai.com
  chat_model: gpt-5.1
  embed_model: text-embedding-3-small
  max_tokens: 4096
  temperature: 0.2

providers:
  warehouse:
    kind: postgres
  catalog:
    enabled: false
  dbt:
    enabled: false
  vector:
    enabled: true
```

## Agent policy

The KB suite uses `DefaultPolicy`, which accepts any well-formed `FinalEnvelope` and does not inject prelude lines or trigger interrupts. The KB workflow is single-phase with no approval gates.

## Example interaction

```json
{
  "v": 1,
  "type": "new",
  "cid": "...",
  "suiteId": "kb",
  "agentType": "kb",
  "question": "What is our data retention policy?"
}
```

The agent searches the vector store, retrieves relevant chunks, and returns:

```json
{
  "type": "final",
  "thread_id": "...",
  "result": {
    "kind": "kb",
    "payload": {
      "answer": "The data retention policy requires 90-day archival for all PII data and 7-year retention for financial records."
    }
  }
}
```

## Next steps

- [Connectors: LanceDB](../connectors/vector/lancedb.md) — vector store configuration
- [Configuration: LLM](../configuration/llm.md) — embedding model setup
