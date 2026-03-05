# Quick Start

This guide walks through building ReAct, configuring a local server, and running your first agent interaction.

## 1. Build

```bash
cargo build -p react
```

## 2. Create a config file

Copy the example config and edit it for your environment:

```bash
cp src/runtime/config.example.yml my-config.yml
```

A minimal config for local development with BigQuery:

```yaml
version: 1

storage:
  mode: local
  path: ./.react

scope:
  tenant: local
  workspace: dev
  project_id: quickstart

llm:
  provider: OPENAI_COMPAT
  base_url: https://api.openai.com
  chat_model: gpt-5.1
  embed_model: text-embedding-3-small
  max_tokens: 8192
  temperature: 0.2

providers:
  warehouse:
    kind: bigquery
    project: your-gcp-project
    dataset: your-dataset
    location: US

  catalog:
    enabled: true

  dbt:
    enabled: true
    runner: host
    target: bigquery

  vector:
    enabled: true
```

## 3. Set your API key

```bash
export LLM_API_KEY="sk-..."
```

## 4. Start the server

```bash
cargo run -p react -- serve --config my-config.yml --port 8787 --terminal
```

The server starts a WebSocket endpoint at `ws://localhost:8787/`.

## 5. Connect and interact

Open a WebSocket connection to `ws://localhost:8787/` using any WS client (e.g. `websocat`, Postman, or your own app). Send a JSON frame to list available suites:

```json
{
  "v": 1,
  "type": "suites",
  "cid": "00000000-0000-0000-0000-000000000001"
}
```

The server responds with the registered suites and their allowed agent types.

Start a new thread with the Data Engineer suite:

```json
{
  "v": 1,
  "type": "new",
  "cid": "00000000-0000-0000-0000-000000000002",
  "suiteId": "data_engineer",
  "agentType": "agent",
  "question": "What tables are available in my dataset?"
}
```

The server responds with `thread_assigned`, then streams `phase`, `tool_start`, `tool_end`, `llm_start`, `llm_end` events as the agent works, and finishes with a `final` frame containing the result.

## What just happened?

1. **serve** — started the WebSocket server, loaded config, and initialised providers (warehouse, catalog, dbt, vector, storage)
2. **suites** — returned the registered suites (`data_engineer`, `kb`) and their allowed agent modes
3. **new** — created a thread, ran the Data Engineer suite's preflight (catalog discovery), then entered the ReAct loop: LLM call → tool execution → observation → repeat until final
4. **final** — the agent produced a validated result, persisted the thread to storage, and streamed the answer back

## Next steps

- [How It Works](../concepts/how-it-works.md) — the ReAct loop, suites, and agent policy
- [Configuration Overview](../configuration/overview.md) — all config blocks and environment variables
- [CLI Reference: serve](../cli/serve.md) — server flags and options
- [WebSocket API](../websocket-api/overview.md) — frame types and connection lifecycle
- [Headless Mode](quickstart-headless.md) — run without a WebSocket connection
