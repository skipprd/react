# Extract and Load: skippr-dbt + skippr

## Architecture

skippr-dbt orchestrates skippr for EL the same way it orchestrates dbt for modeling: generates configuration, invokes the CLI, reads structured output, reasons about results. No shared code between the two projects — only CLI invocation and config/env var passing.

**Key principle:** skippr always owns source discovery; skippr-dbt always owns the mapping.

```text
source system -> [skippr discover] -> schemas -> [skippr-dbt LLM mapping] -> [skippr sync] -> destination warehouse -> [dbt]
```

## How It Works

### 1. Configuration

Users configure `providers.el` alongside the existing `providers.warehouse`:

```yaml
providers:
  warehouse:                    # destination (existing, unchanged)
    kind: snowflake
    database: ANALYTICS
    schema: RAW

  el:                           # EL tool configuration (skippr, airbyte, singer, etc.)
    enabled: true
    skippr_binary: skippr       # path to skippr CLI (default: "skippr")
    skippr_input:               # opaque block — passed through to skippr.yml
      kind: mssql
      connection_string: "${MSSQL_CONNECTION_STRING}"
      tables:                   # optional — omit to discover all
        - dbo.customers
        - dbo.orders
```

- `providers.warehouse` is always the destination
- `providers.el` is NOT a warehouse provider — skippr-dbt never connects to the source
- `skippr_input` is opaque config passed through to skippr's `data_inputs` section

### 2. Workflow Phases

When `el` is configured, five EL phases prepend the existing agent workflow:

```
ElDiscover -> ElMapPlan -> ElMapReview -> ElSync -> ElVerify -> Preflight -> CleansePlan -> ...
```

When `el` is absent, the workflow starts at `Preflight` with zero behavior change.

| Phase | What happens | LLM? |
|-------|-------------|------|
| **ElDiscover** | Generate skippr.yml, invoke `skippr discover`, read schemas via `SHOW PIPELINE` | No |
| **ElMapPlan** | LLM produces destination schemas per source table (skeleton, enrichment, design memo, critique) | Yes |
| **ElMapReview** | Batched LLM review of the mapping plan | Yes |
| **ElSync** | Write schemas via `LOAD SCHEMA`, invoke `skippr sync --once --output json` | No |
| **ElVerify** | Verify destination tables via warehouse provider + `SHOW PIPELINE` | No |

### 3. skippr Interaction

skippr-dbt interacts with skippr through four interfaces:

1. **Config file** — `skippr.yml` written to the workspace data directory
2. **Discover** — `skippr discover --pipeline <name> --output json`
3. **DDL commands** — `skippr query --plain --sql "SHOW PIPELINE <name>"` and `LOAD SCHEMA`
4. **Sync** — `skippr sync --pipeline <name> --once --output json`

All skippr invocations run with `SKIPPR_STORAGE_MODE=local`. Auth credentials are passed as env vars.

## skippr-side Capabilities (Implemented)

- **MSSQL Input Plugin** — `DATA_SOURCE_PLUGIN_NAME=Mssql`; optional `tables` for auto-discovery; namespace `mssql.{db}.{schema}.{table}`
- **Snowflake Output Plugin** — `DATA_OUTPUT_PLUGIN_NAME=Snowflake`; namespace-to-table: dots to underscores, lowercased
- **`LOAD SCHEMA` DDL** — writes LLM-produced destination schemas to skippr pipeline metadata
- **`SHOW PIPELINE` DDL** — returns namespaces, field schemas, offsets as JSON
- **Output Modes** — `--output progress|json|text` on `sync` and `discover`
- **Batch Sync** — `--once` flag for single-pass execution
- **Local Metadata** — `SKIPPR_STORAGE_MODE=local` for disk-only metadata persistence

## Scope

### In Scope

- Batch-style table-to-table extraction and load
- LLM-assisted schema mapping (source -> destination)
- Preparing raw/bronze destination tables for dbt consumption
- MSSQL source, Snowflake destination (initial)
- Incremental loads via skippr's internal offset tracking

### Out of Scope

- Replacing dbt
- Real-time streaming (already handled by skippr for supported sources)
- CDC, Kafka, or API source support
- skippr-dbt connecting directly to source systems
