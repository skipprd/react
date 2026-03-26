# How It Works

Skippr dbt orchestrates two tools to build a complete data pipeline:

- **skippr** -- a local CLI that handles extract-and-load (reading from sources, writing to warehouses)
- **dbt** -- the industry-standard tool for SQL-based data transformation

The orchestrator connects them with an AI-assisted schema mapping layer.

## Pipeline flow

```
Source (MSSQL, S3)
  |
  |  discover -- reads source schemas
  v
Schema Discovery
  |
  |  AI-assisted mapping -- designs destination schemas
  v
Schema Mapping
  |
  |  sync -- extracts rows, loads into warehouse
  v
Bronze Tables (Snowflake, BigQuery, or Postgres)
  |
  |  dbt -- generates and runs silver/gold models
  v
Silver Models (staging: cleaned, typed, renamed)
  |
  v
Gold Models (marts: business-ready aggregations)
```

## Data flow

All data transfer is local:

- Source data is read on your machine.
- Data is written directly to the warehouse over the network (Snowflake REST API, BigQuery API, Postgres wire protocol, etc.).
- No data passes through any third-party service.

The LLM is used only for schema mapping decisions (column naming, type inference, model structure). It receives table names and column metadata, not row-level data.

## Naming convention

The pipeline creates schemas in the warehouse using the project name:

| Tier | Schema name | Contents |
|---|---|---|
| Bronze | `<warehouse_schema>` (e.g. `RAW`) | Raw extracted data |
| Silver | `<project>_silver` (e.g. `mssql_migration_silver`) | Staged, cleaned, typed |
| Gold | `<project>_gold` (e.g. `mssql_migration_gold`) | Business-ready models |

## Incremental runs

Re-running `skippr-dbt run` on an existing project is incremental:

- **Extract-and-load**: `skippr` tracks offsets internally and only syncs new/changed rows.
- **dbt models**: existing models are preserved; the agent updates or adds new models as needed.

## Local artifacts

Pipeline state, logs, and intermediate files are stored under `.skippr-dbt/` in the working directory. See [Logs and Artifacts](operations/logs.md) for details.
