# Data Engineer Suite

The Data Engineer suite (`data_engineer`) is the primary product surface for analytics and dbt workflows. It takes a user question or task, discovers available datasets, builds an execution plan, authors dbt models, validates them, runs a review pass, and publishes to the warehouse.

## Suite ID and agent types

| Property | Value |
|---|---|
| Suite ID | `data_engineer` |
| Agent types | `ask`, `agent`, `review` |
| Default agent type | `agent` |

### Agent type behaviour

| Agent type | Behaviour |
|---|---|
| `ask` | Answer a data question using SQL. No dbt authoring. Returns a `final` with kind `ask` (answer + SQL + optional data/chart). |
| `agent` | Full workflow: preflight → plan → author → validate → review → publish. Creates and validates dbt models. |
| `review` | Red-team review of authored models. Returns `review` frames with decision metadata. |

## Tools

The Data Engineer suite registers a rich set of tools. Key categories:

| Category | Tools | Purpose |
|---|---|---|
| **SQL** | `sql_run`, `sql_sample`, `sql_schema`, `sql_stats` | Execute SQL, sample data, inspect schemas, compute column stats |
| **dbt authoring** | `staging_model`, `gold_model`, `apply_next_batch`, `apply_next_schema_batch` | Author staging/cleansing/gold dbt models via batch patches |
| **dbt validation** | `dbt_validate`, `dbt_files` | Validate dbt projects, inspect generated files |
| **dbt repair** | `dbt_repair` | Automatically remediate dbt validation failures |
| **dbt publish** | `publish_dbt_to_provider` | Publish validated dbt models to the warehouse |
| **Catalog** | `catalog_note`, `sql_register` | Annotate catalog entries, register SQL results |
| **Vector** | `vect_query`, `vect_upsert` | Semantic search and embedding management |
| **Artifacts** | `artifacts`, `json_file`, `dbt_examples` | Read/write artifact files, inspect dbt example projects |
| **User interaction** | `ask_user`, `ask_approval` | Request user input or binary approval |
| **Plan** | Plan prompt helpers | Assist with plan creation and grounding |

## Preflight

Before the agent loop starts, the Data Engineer suite runs a preflight phase that:

1. Discovers available datasets from the warehouse (via `DatasetCatalogProvider`)
2. Refreshes the catalog metadata (column descriptions, types, row counts)
3. Indexes catalog entries into the vector store for semantic search
4. Builds dataset candidate lists for plan grounding

This ensures the agent has up-to-date knowledge of the data landscape before it begins reasoning.

## Plans

The `agent` mode uses structured execution plans. A plan contains:

- **Tasks** — one per dataset or deliverable (e.g. "Staging model for orders", "Gold metric for weekly rides")
- **Checklist items** — sub-steps within each task (e.g. `sql_model`, `schema_contract`, `validate`)
- **Work groups** — ordered groups of checklist items that the agent executes sequentially

Plans are persisted and sent to the client via `plans` and `plans_changed` frames so the UI can render progress.

## Configuration

The Data Engineer suite uses these provider config blocks:

- `providers.warehouse` — which warehouse to query (required)
- `providers.catalog` — catalog refresh settings
- `providers.dbt` — dbt runner, target, naming conventions
- `providers.vector` — vector store for semantic search

See [Configuration: Providers](../configuration/providers.md) for details.

## Supported warehouses

The Data Engineer suite supports the following warehouse providers:

| Warehouse | Config `kind` | dbt adapter | Connector docs |
|---|---|---|---|
| AWS Athena | `athena` | `dbt-athena-community` | [Athena](../connectors/warehouses/athena.md) |
| Google BigQuery | `bigquery` | `dbt-bigquery` | [BigQuery](../connectors/warehouses/bigquery.md) |
| Microsoft SQL Server | `mssql` | `dbt-sqlserver` | [MSSQL](../connectors/warehouses/mssql.md) |
| PostgreSQL | `postgres` | `dbt-postgres` | [Postgres](../connectors/warehouses/postgres.md) |
| Snowflake | `snowflake` | `dbt-snowflake` | [Snowflake](../connectors/warehouses/snowflake.md) |

## Next steps

- [Data Engineer Phases](data-engineer-phases.md) — detailed walkthrough of each phase
- [Configuration: Providers](../configuration/providers.md) — warehouse, catalog, dbt, vector config
- [MSSQL → Snowflake Getting Started](../../getting-started.md) — end-to-end migration guide
