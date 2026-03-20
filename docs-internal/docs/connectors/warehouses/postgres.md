# Postgres

The Postgres provider uses PostgreSQL for SQL execution and schema discovery. This is the `provider-postgres` module (`src/modules/provider-postgres/`).

## Configuration

```yaml
providers:
  warehouse:
    kind: postgres
    database: mydb
    schema: public
```

| Field | YAML path | Default | Description |
|---|---|---|---|
| `database` | `providers.warehouse.database` | *(empty)* | Database name |
| `schema` | `providers.warehouse.schema` | *(empty)* | Default schema for discovery and unqualified references |

## Connection

The Postgres provider uses standard PostgreSQL connection parameters from environment variables:

```bash
export PGHOST=localhost
export PGPORT=5432
export PGUSER=myuser
export PGPASSWORD=mypassword
export PGDATABASE=mydb
```

Alternatively, use a connection string:

```bash
export DATABASE_URL="postgres://myuser:mypassword@localhost:5432/mydb"
```

## Concurrency

The provider uses a semaphore to limit concurrent connections. The default limit matches `DEFAULT_WAREHOUSE_MAX_CONCURRENCY` from the core module.

## dbt target

When using dbt with Postgres, set:

```yaml
providers:
  dbt:
    target: postgres
```

And install the dbt adapter:

```bash
pip install dbt-postgres
```

## Discovery

The Postgres provider discovers tables and views from the configured schema using `information_schema`. It supports:

- Tables
- Views
- Materialized views
- Column types and nullability
