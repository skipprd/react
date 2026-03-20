# ReAct

ReAct is a WebSocket-based agent runtime that drives AI-powered data engineering and knowledge-base workflows. Clients send JSON frames over a WebSocket connection, the server routes each request to a **suite**, and the suite runs a **ReAct loop** (LLM → tool calls → observations → final/interrupt) using injected providers for query execution, catalog management, vector search, dbt, and storage.

## Key capabilities

- **ReAct agent loop** — strict JSON action parsing (tool calls and finals) with configurable step budgets, per-tool timeouts, and policy-driven interrupt/approval gates
- **Suite-based architecture** — each product surface (Data Engineer, Knowledge Base) defines its own tool registry, system prompts, agent policy, and optional preflight behaviour
- **Multi-tenant scoping** — all persistence is namespaced by `tenant / workspace / project_id`, supporting isolated workloads on shared infrastructure
- **Pluggable warehouses** — Athena, BigQuery, MSSQL, Postgres, and Snowflake providers implement a common `QueryProvider` trait for SQL execution and schema discovery
- **dbt integration** — automated project scaffolding, model authoring, validation, and publishing via a shelled-out dbt runner (host or Docker)
- **Vector search** — LanceDB-backed embeddings for semantic catalog search and knowledge-base retrieval
- **Flexible storage** — local filesystem or S3 for threads, artifacts, catalog, and vector indices
- **WebSocket + CLI** — interactive server mode (`react serve`) and non-interactive headless mode (`react run`) with parallel multi-config execution

## How it works

```
Client (WebSocket)
  │
  ▼
WS Server ─── parse frame ───▶ Suite Router
                                    │
                                    ▼
                               Suite (tools + prompts + policy)
                                    │
                                    ▼
                               ReAct Loop
                                ┌───┴───┐
                                ▼       ▼
                           LLM Call   Tool Exec
                                │       │
                                └───┬───┘
                                    ▼
                              Final / Interrupt
                                    │
                                    ▼
                              Thread Persistence
                                    │
                                    ▼
                              WS Response Frames
```

## Quick start

Build and run your first ReAct server in under 5 minutes. See the [Quick Start](getting-started/quickstart.md) guide.

## License

ReAct is licensed under the [Elastic License 2.0 (ELv2)](license.md).
