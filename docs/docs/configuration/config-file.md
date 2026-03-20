# Config File

Skippr dbt is configured via `skippr-dbt.yaml` in the working directory.

## Format

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

dbt:
  target_schema: custom_name
  silver_suffix: silver
  gold_suffix: gold
```

## Fields

### project (required)

The project identifier. Used as:

- The pipeline name for extract-and-load
- The default dbt schema prefix (e.g. `mssql_migration_silver`, `mssql_migration_gold`)

### warehouse (required)

The destination warehouse connection.

#### Snowflake

| Field | Description |
|---|---|
| `kind` | `snowflake` |
| `database` | Snowflake database |
| `schema` | Bronze/raw schema where extracted data lands |
| `warehouse` | Compute warehouse |
| `role` | Snowflake role |

Authentication is via environment variables, not the config file.

#### BigQuery

| Field | Description |
|---|---|
| `kind` | `bigquery` |
| `project` | GCP project ID |
| `dataset` | BigQuery dataset |
| `location` | Dataset location |

### source (optional)

The data source for extraction. When absent, the pipeline skips extraction and starts at the modeling phase.

#### MSSQL

| Field | Description |
|---|---|
| `kind` | `mssql` |
| `connection_string` | ADO.NET connection string. Use `${ENV_VAR}` to reference environment variables. |

#### S3

| Field | Description |
|---|---|
| `kind` | `s3` |
| `s3_bucket` | S3 bucket name |
| `s3_prefix` | Key prefix |

### dbt (optional)

Override dbt naming conventions.

| Field | Default | Description |
|---|---|---|
| `target_schema` | Same as `project` | Base schema name for dbt materialisation |
| `silver_suffix` | `silver` | Suffix for silver/staging tier |
| `gold_suffix` | `gold` | Suffix for gold/mart tier |

With `project: analytics`, dbt materialises to:

- `analytics_silver` (staging models)
- `analytics_gold` (mart models)
