# Snowflake

The Snowflake provider uses Snowflake as the warehouse for SQL execution, schema discovery, and dbt materialisation.

## Configuration

```yaml
providers:
  warehouse:
    kind: snowflake
    database: ANALYTICS
    schema: RAW
    warehouse: COMPUTE_WH
    role: TRANSFORMER
```

| Field | YAML path | Default | Description |
|---|---|---|---|
| `database` | `providers.warehouse.database` | `SNOWFLAKE_DATABASE` env var | Snowflake database name |
| `schema` | `providers.warehouse.schema` | *(empty)* | Default schema for discovery (bronze/raw) |
| `warehouse` | `providers.warehouse.warehouse` | `SNOWFLAKE_WAREHOUSE` env var | Snowflake compute warehouse |
| `role` | `providers.warehouse.role` | `SNOWFLAKE_ROLE` env var | Snowflake role for the session |

If `database`, `warehouse`, or `role` are omitted from the YAML, the runtime falls back to their respective environment variables.

## Authentication

Snowflake credentials are provided exclusively via environment variables. They are injected into the generated dbt `profiles.yml` using Jinja `env_var()` calls and are never written to disk in plaintext.

```bash
export SNOWFLAKE_ACCOUNT="xy12345.us-east-1"
export SNOWFLAKE_USER="your_username"
export SNOWFLAKE_PASSWORD="your_password"
```

| Variable | Required | Description |
|---|---|---|
| `SNOWFLAKE_ACCOUNT` | Yes | Account identifier including region (e.g. `xy12345.us-east-1`) |
| `SNOWFLAKE_USER` | Yes | Login username |
| `SNOWFLAKE_PASSWORD` | Yes | Login password |
| `SNOWFLAKE_DATABASE` | No | Fallback if `database` is omitted from YAML |
| `SNOWFLAKE_WAREHOUSE` | No | Fallback if `warehouse` is omitted from YAML |
| `SNOWFLAKE_ROLE` | No | Fallback if `role` is omitted from YAML |

## Snowflake permissions

The role used by the agent needs:

| Permission | Purpose |
|---|---|
| USAGE on warehouse | Execute queries |
| USAGE on database | Access database metadata |
| USAGE on raw schema | Discover and query bronze tables |
| CREATE SCHEMA on database | dbt creates silver/gold schemas during materialisation |
| CREATE TABLE / CREATE VIEW on target schemas | dbt materialises models |

Minimal grant example:

```sql
GRANT USAGE ON WAREHOUSE COMPUTE_WH TO ROLE TRANSFORMER;
GRANT USAGE ON DATABASE ANALYTICS TO ROLE TRANSFORMER;
GRANT USAGE ON SCHEMA ANALYTICS.RAW TO ROLE TRANSFORMER;
GRANT SELECT ON ALL TABLES IN SCHEMA ANALYTICS.RAW TO ROLE TRANSFORMER;
GRANT CREATE SCHEMA ON DATABASE ANALYTICS TO ROLE TRANSFORMER;
```

## dbt target

When using dbt with Snowflake, set:

```yaml
providers:
  dbt:
    target: snowflake
```

And install the dbt adapter:

```bash
pip install dbt-snowflake
```

The runtime generates a dbt `profiles.yml` with the following structure:

```yaml
<project_id>:
  target: snowflake
  outputs:
    snowflake:
      type: snowflake
      account: "{{ env_var('SNOWFLAKE_ACCOUNT') }}"
      user: "{{ env_var('SNOWFLAKE_USER') }}"
      password: "{{ env_var('SNOWFLAKE_PASSWORD') }}"
      role: TRANSFORMER
      database: ANALYTICS
      warehouse: COMPUTE_WH
      schema: <target_schema>
```

The `schema` value is derived from `providers.dbt.naming.target_schema` (or `scope.project_id` if not set). dbt appends tier suffixes to produce the final schema names (e.g. `myproject_silver`, `myproject_gold`).

## Schema naming

With the default naming strategy, dbt materialises into:

| Tier | Schema |
|---|---|
| Silver (staging) | `<target_schema>_<silver_suffix>` |
| Gold (mart) | `<target_schema>_<gold_suffix>` |

Example with `target_schema: mssql_migration`, `silver_suffix: silver`, `gold_suffix: gold`:

| Tier | Schema |
|---|---|
| Silver | `MSSQL_MIGRATION_SILVER` |
| Gold | `MSSQL_MIGRATION_GOLD` |

## Full example

```yaml
version: 1

storage:
  mode: local
  path: ./.react

scope:
  tenant: local
  workspace: dev
  project_id: mssql_migration

llm:
  provider: OPENAI_COMPAT
  base_url: https://api.openai.com
  reason_model: gpt-5-mini
  task_model: gpt-5-mini
  embed_model: text-embedding-3-small
  max_tokens: 8192
  temperature: 0.2

providers:
  warehouse:
    kind: snowflake
    database: ANALYTICS
    schema: RAW
    warehouse: COMPUTE_WH
    role: TRANSFORMER

  catalog:
    enabled: true
    refresh_secs: 3600
    max_concurrency: 8

  dbt:
    enabled: true
    runner: host
    target: snowflake
    naming:
      target_schema: mssql_migration
      silver_suffix: silver
      gold_suffix: gold

  vector:
    enabled: true
```

## Migrating from MSSQL

For a step-by-step guide on exporting MSSQL data into Snowflake as the bronze tier and running the agent, see [`getting-started.md`](../../../getting-started.md) in the repository root.
