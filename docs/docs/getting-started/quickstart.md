# Quick Start

This guide walks through setting up a project that extracts data from MSSQL, loads it into Snowflake, and generates dbt models -- in under 5 minutes.

## Prerequisites

- `skippr` on PATH ([Install](install.md))
- Python venv with `dbt-core` and `dbt-snowflake` installed
- Authenticated session (`skippr user login`) or `SKIPPR_API_KEY` set
- Environment variables set: `SNOWFLAKE_ACCOUNT`, `SNOWFLAKE_USER`, `SNOWFLAKE_PRIVATE_KEY_PATH`
- An accessible MSSQL instance (or Docker for local dev)

## 1. Initialise the project

```bash
mkdir my-workspace && cd my-workspace
skippr init mssql-migration
```

This creates `skippr.yaml` with your project name and a `.env.example` showing the required environment variables.

## 2. Connect the warehouse

```bash
skippr connect warehouse snowflake \
  --database ANALYTICS \
  --schema RAW \
  --warehouse COMPUTE_WH \
  --role ACCOUNTADMIN
```

Or run without flags to be prompted interactively.

## 3. Connect the source

```bash
skippr connect source mssql \
  --connection-string '${MSSQL_CONNECTION_STRING}'
```

Using `${MSSQL_CONNECTION_STRING}` reads the value from your environment at runtime. Set it:

```bash
export MSSQL_CONNECTION_STRING="server=tcp:127.0.0.1,1433;database=testdb;user id=sa;password=YourPass;TrustServerCertificate=true"
```

## 4. Check prerequisites

```bash
skippr doctor
```

This verifies that all binaries, credentials, and config are in place.

## 5. Run the pipeline

```bash
skippr run
```

The pipeline will:

1. **Discover** source schemas from MSSQL.
2. **Sync** data into Snowflake bronze tables.
3. **Verify** the destination tables are queryable.
4. **Plan** a silver (staging) layer with one model per raw table.
5. **Author** dbt SQL models with type casting and column mapping.
6. **Validate** by running `dbt compile` and `dbt run` against the warehouse.

## 6. Verify outputs

### Generated config

```
skippr.yaml          # your project config
```

### Local artifacts

```
.skippr/
└── local/
    └── dev/
        └── mssql_migration/
            ├── logs/        # run logs
            └── pipeline/    # generated pipeline config
```

### dbt models

```
models/
├── schema.yml                   # source definitions
└── staging/
    ├── stg_raw_customers.sql    # silver model
    └── stg_raw_orders.sql       # silver model
```

### Snowflake schemas

| Schema | Contents |
|---|---|
| `ANALYTICS.RAW` | Bronze -- raw MSSQL data |
| `ANALYTICS.MSSQL_MIGRATION_SILVER` | Silver -- staged and cleansed |
| `ANALYTICS.MSSQL_MIGRATION_GOLD` | Gold -- mart-ready models |

## What's in skippr.yaml

After running `init` and `connect`, your config looks like this:

```yaml
project: mssql_migration

warehouse:
  kind: snowflake
  database: ANALYTICS
  schema: RAW
  warehouse: COMPUTE_WH
  role: ACCOUNTADMIN

source:
  kind: mssql
  connection_string: ${MSSQL_CONNECTION_STRING}
```

That's the entire user-facing config. Everything else is handled automatically.
