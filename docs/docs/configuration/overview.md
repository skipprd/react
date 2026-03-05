# Configuration Overview

ReAct is configured via a YAML config file loaded with `--config <path>`. Environment variables and CLI flags can override any value.

## Precedence

Values are resolved in this order (highest priority first):

1. **CLI flag** (e.g. `--port 9090`)
2. **Environment variable** (e.g. `LLM_CHAT_MODEL=gpt-5.1`)
3. **YAML config file**
4. **Hardcoded default**

## Config file format

```yaml
version: 1

server:
  port: 8787

storage:
  mode: local
  path: ./.react

scope:
  tenant: default
  workspace: default
  project_id: default

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
    project: my-project
    dataset: my-dataset
  catalog:
    enabled: true
  dbt:
    enabled: true
    runner: host
    target: bigquery
  vector:
    enabled: true
```

## Quick reference

| Block | Section | Description |
|---|---|---|
| `server` | [Server](server.md) | Port and server settings |
| `storage` | [Storage](storage.md) | Storage backend (local vs S3) and paths |
| `scope` | [Server](server.md) | Tenant, workspace, project_id for multi-tenant scoping |
| `llm` | [LLM](llm.md) | LLM provider, model, token limits, temperature |
| `providers.warehouse` | [Providers](providers.md) | Warehouse connection (Athena, BigQuery, Postgres, etc.) |
| `providers.catalog` | [Providers](providers.md) | Catalog refresh settings |
| `providers.dbt` | [Providers](providers.md) | dbt runner, target, naming conventions |
| `providers.vector` | [Providers](providers.md) | Vector store toggle |

## Environment variables

All environment variables are listed in [Environment Variables](environment-variables.md).

Secrets (API keys, credentials) are always configured via environment variables, never in YAML:

```bash
export LLM_API_KEY="sk-..."
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/service-account.json"
```
