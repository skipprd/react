# Snowflake

The Snowflake provider uses Snowflake for SQL execution and dataset discovery. This is the `provider-snowflake` module (`src/modules/provider-snowflake/`).

## Configuration

```yaml
providers:
  warehouse:
    kind: snowflake
    database: ANALYTICS
    schema: RAW
    warehouse: COMPUTE_WH
    role: ANALYST
    max_concurrency: 15
    discovery_cache_ttl_secs: 120
```

| Field | YAML path | Default | Description |
|---|---|---|---|
| `database` | `providers.warehouse.database` | *(empty)* | Snowflake database name |
| `schema` | `providers.warehouse.schema` | *(empty)* | Default schema for discovery and unqualified references |
| `warehouse` | `providers.warehouse.warehouse` | *(none)* | Snowflake virtual warehouse for compute |
| `role` | `providers.warehouse.role` | *(none)* | Snowflake role to use for the session |
| `max_concurrency` | `providers.warehouse.max_concurrency` | `15` | Max concurrent Snowflake queries |
| `discovery_cache_ttl_secs` | `providers.warehouse.discovery_cache_ttl_secs` | `120` | TTL for cached discovery results (seconds) |

## Authentication

Two authentication methods are supported. **Key-pair auth is recommended** — it bypasses MFA and is the standard approach for programmatic/service access.

### Key-pair authentication (recommended)

Key-pair auth uses an RSA private key assigned to the Snowflake user. MFA is not triggered.

**1. Generate an RSA key pair (one-time setup):**

```bash
openssl genrsa 2048 | openssl pkcs8 -topk8 -inform PEM -out snowflake_key.p8 -nocrypt
openssl rsa -in snowflake_key.p8 -pubout -out snowflake_key.pub
```

**2. Assign the public key to your Snowflake user:**

```sql
ALTER USER MYUSER SET RSA_PUBLIC_KEY='MIIBIjANBgkqh...';
```

Copy the contents of `snowflake_key.pub` **without** the `-----BEGIN/END PUBLIC KEY-----` lines.

**3. Set environment variables:**

```bash
export SNOWFLAKE_ACCOUNT=myorg-myaccount
export SNOWFLAKE_USER=myuser
export SNOWFLAKE_PRIVATE_KEY_PATH=/path/to/snowflake_key.p8
```

### Password authentication

Password auth works when MFA is **not** enforced on the Snowflake account:

```bash
export SNOWFLAKE_ACCOUNT=xy12345.us-east-1
export SNOWFLAKE_USER=myuser
export SNOWFLAKE_PASSWORD=mypassword
```

> **Note:** If your Snowflake account has MFA enabled, password auth will fail with error code `390197`. Switch to key-pair auth above.

### Optional overrides

These can also be set in the YAML config:

```bash
export SNOWFLAKE_WAREHOUSE=COMPUTE_WH
export SNOWFLAKE_ROLE=ANALYST
export SNOWFLAKE_DATABASE=ANALYTICS
export SNOWFLAKE_SCHEMA=RAW
```

The `SNOWFLAKE_ACCOUNT` value is the Snowflake account identifier (e.g. `xy12345.us-east-1` or `myorg-myaccount`).

## Snowflake permissions

The role used by ReAct needs:

| Permission | Purpose |
|---|---|
| `USAGE` on warehouse | Execute queries |
| `USAGE` on database | Access database metadata |
| `USAGE` on schema(s) | Access schema metadata and tables |
| `SELECT` on tables/views | Read source data for discovery and sampling |
| `CREATE SCHEMA` (optional) | dbt materialisation into new schemas |
| `CREATE TABLE` (optional) | dbt materialisation |

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

## SQL dialect

Snowflake SQL supports:

- `VARIANT`, `OBJECT`, and `ARRAY` types for semi-structured data
- `FLATTEN()` for unnesting arrays and objects
- `QUALIFY` clause for window function filtering
- `TRY_CAST()` for safe type conversion
- Case-insensitive identifiers (unless double-quoted)
- `LIMIT` syntax (same as PostgreSQL)

## Discovery

The Snowflake provider discovers tables and views using `INFORMATION_SCHEMA`:

- Schemas from `INFORMATION_SCHEMA.SCHEMATA`
- Tables and views from `INFORMATION_SCHEMA.TABLES`
- Column metadata from `INFORMATION_SCHEMA.COLUMNS`

When a `schema` is configured, discovery is scoped to that single schema. Otherwise all non-system schemas in the database are discovered.

## Stats

Per-field statistics are collected using:

- `COUNT(*)` for row count
- `APPROX_COUNT_DISTINCT()` for approximate distinct counts
- `MIN()` / `MAX()` for numeric range (timestamps converted via `EXTRACT(EPOCH FROM ...)`)
- Null counts via `SUM(CASE WHEN col IS NULL THEN 1 ELSE 0 END)`

Complex types (`VARIANT`, `OBJECT`, `ARRAY`) only collect null counts.
