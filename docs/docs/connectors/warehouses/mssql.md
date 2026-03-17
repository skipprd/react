# Microsoft SQL Server (MSSQL)

The MSSQL provider uses Microsoft SQL Server as the warehouse for SQL execution, schema discovery, and dbt materialisation. The dbt integration uses the `dbt-sqlserver` adapter with ODBC Driver 18.

## Configuration

```yaml
providers:
  warehouse:
    kind: mssql
    database: MyDatabase
    schema: dbo
```

| Field | YAML path | Default | Description |
|---|---|---|---|
| `database` | `providers.warehouse.database` | `MSSQL_DATABASE` env var | SQL Server database name |
| `schema` | `providers.warehouse.schema` | *(empty)* | Default schema for discovery (e.g. `dbo`) |

## Connection

MSSQL credentials are provided via environment variables. They are injected into the generated dbt `profiles.yml` using Jinja `env_var()` calls.

```bash
export MSSQL_HOST="localhost"
export MSSQL_USER="sa"
export MSSQL_PASSWORD="YourStrong!Passw0rd"
export MSSQL_DATABASE="MyDatabase"    # fallback if database is omitted from YAML
```

| Variable | Required | Description |
|---|---|---|
| `MSSQL_HOST` | Yes | SQL Server hostname or IP |
| `MSSQL_USER` | Yes | Login username |
| `MSSQL_PASSWORD` | Yes | Login password |
| `MSSQL_DATABASE` | No | Fallback if `database` is omitted from YAML |

The generated dbt profile connects on port `1433` using `ODBC Driver 18 for SQL Server`.

## ODBC driver

The `dbt-sqlserver` adapter requires an ODBC driver. Install ODBC Driver 18 for your platform:

```bash
# macOS
brew tap microsoft/mssql-release https://github.com/microsoft/homebrew-mssql-release
brew install msodbcsql18

# Ubuntu/Debian
curl https://packages.microsoft.com/keys/microsoft.asc | sudo apt-key add -
sudo add-apt-repository "$(curl https://packages.microsoft.com/config/ubuntu/$(lsb_release -rs)/prod.list)"
sudo apt-get update
sudo apt-get install -y msodbcsql18

# Windows
# Download from https://learn.microsoft.com/en-us/sql/connect/odbc/download-odbc-driver-for-sql-server
```

## dbt target

When using dbt with MSSQL, set:

```yaml
providers:
  dbt:
    target: sqlserver
```

And install the dbt adapter:

```bash
pip install dbt-sqlserver
```

The runtime generates a dbt `profiles.yml` with the following structure:

```yaml
<project_id>:
  target: sqlserver
  outputs:
    sqlserver:
      type: sqlserver
      driver: 'ODBC Driver 18 for SQL Server'
      server: "{{ env_var('MSSQL_HOST') }}"
      port: 1433
      database: MyDatabase
      schema: <target_schema>
      user: "{{ env_var('MSSQL_USER') }}"
      password: "{{ env_var('MSSQL_PASSWORD') }}"
```

## SQL Server permissions

The login used by the agent needs:

| Permission | Purpose |
|---|---|
| `db_datareader` on source database | Read source tables for discovery and querying |
| `db_ddladmin` on target schemas | dbt creates tables/views during materialisation |
| `CREATE SCHEMA` | dbt creates silver/gold schemas |

## Full example

```yaml
version: 1

storage:
  mode: local
  path: ./.react

scope:
  tenant: local
  workspace: dev
  project_id: mssql_analytics

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
    kind: mssql
    database: MyDatabase
    schema: dbo

  catalog:
    enabled: true
    refresh_secs: 3600
    max_concurrency: 8

  dbt:
    enabled: true
    runner: host
    target: sqlserver
    naming:
      target_schema: mssql_analytics
      silver_suffix: silver
      gold_suffix: gold

  vector:
    enabled: true
```

## Common use case: MSSQL as a source for Snowflake

MSSQL is commonly used as the source system in migration workflows. For a guide on exporting MSSQL data into Snowflake as the bronze tier and building silver/gold models, see [`getting-started.md`](../../../getting-started.md) in the repository root.
