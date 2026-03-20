# Microsoft SQL Server

The MSSQL provider uses Microsoft SQL Server for SQL execution and dataset discovery. This is the `provider-mssql` module (`src/modules/provider-mssql/`).

## Configuration

```yaml
providers:
  warehouse:
    kind: mssql
    database: mydb
    schema: dbo
    max_concurrency: 15
    discovery_cache_ttl_secs: 120
```

| Field | YAML path | Default | Description |
|---|---|---|---|
| `database` | `providers.warehouse.database` | *(empty)* | SQL Server database name |
| `schema` | `providers.warehouse.schema` | `dbo` | Default schema for discovery and unqualified references |
| `max_concurrency` | `providers.warehouse.max_concurrency` | `15` | Max concurrent SQL Server queries |
| `discovery_cache_ttl_secs` | `providers.warehouse.discovery_cache_ttl_secs` | `120` | TTL for cached discovery results (seconds) |

## Connection

The MSSQL provider uses environment variables for connection details:

```bash
export MSSQL_HOST=localhost
export MSSQL_PORT=1433
export MSSQL_USER=sa
export MSSQL_PASSWORD=MyStr0ngP@ssword
export MSSQL_DATABASE=mydb
```

### TLS / certificate trust

By default the provider validates the server certificate. For development or self-signed certificates:

```bash
export MSSQL_TRUST_CERT=true
```

## SQL Server permissions

The login used by ReAct needs:

| Permission | Purpose |
|---|---|
| `db_datareader` role | Read tables and views for discovery and sampling |
| `SELECT` on `INFORMATION_SCHEMA` | Schema and table discovery |
| `db_datawriter` role (optional) | dbt materialisation |
| `db_ddladmin` role (optional) | dbt schema/table creation |

## dbt target

When using dbt with SQL Server, set:

```yaml
providers:
  dbt:
    target: sqlserver
```

And install the dbt adapter:

```bash
pip install dbt-sqlserver
```

The dbt profile uses ODBC Driver 18 for SQL Server. Ensure the ODBC driver is installed on the host or Docker image.

## SQL dialect (T-SQL)

T-SQL differences from standard SQL:

- **Row limiting**: Use `SELECT TOP N` instead of `LIMIT N`
- **Identifier quoting**: Square brackets `[schema].[table]` (not double quotes)
- **Current time**: `GETDATE()` or `SYSDATETIME()` instead of `NOW()`
- **String concatenation**: `+` operator or `CONCAT()` (not `||`)
- **Safe cast**: `TRY_CAST(expr AS type)` returns NULL on failure
- **Booleans**: `BIT` type with `0`/`1` (no `TRUE`/`FALSE` literals)
- **Unicode strings**: Prefix with `N` (e.g. `N'text'`), use `NVARCHAR` type

## Discovery

The MSSQL provider discovers tables and views using `INFORMATION_SCHEMA`:

- Schemas from `INFORMATION_SCHEMA.SCHEMATA`
- Tables and views from `INFORMATION_SCHEMA.TABLES`
- Column metadata from `INFORMATION_SCHEMA.COLUMNS`

When a `schema` is configured (and is not `dbo`), discovery is scoped to that single schema. Otherwise all non-system schemas are discovered.

## Stats

Per-field statistics are collected using:

- `COUNT(*)` for row count
- `COUNT(DISTINCT col)` for exact distinct counts
- `MIN()` / `MAX()` for numeric range (dates converted via `DATEDIFF(SECOND, '19700101', col)`)
- Null counts via `SUM(CASE WHEN col IS NULL THEN 1 ELSE 0 END)`

Complex types (`xml`, `geography`, `geometry`) only collect null counts.
