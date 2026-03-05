# Providers

The `providers` block configures the warehouse, catalog, dbt, and vector capabilities injected into suites.

## providers.warehouse

The warehouse provider is required. It determines which database backend is used for SQL execution and dataset discovery.

| | |
|---|---|
| **YAML path** | `providers.warehouse` |
| **Required** | Yes |

The warehouse block uses a tagged union on the `kind` field:

| Kind | Description | See |
|---|---|---|
| `athena` | AWS Athena + Glue | [Athena connector](../connectors/warehouses/athena.md) |
| `bigquery` | Google BigQuery | [BigQuery connector](../connectors/warehouses/bigquery.md) |
| `postgres` | PostgreSQL | [Postgres connector](../connectors/warehouses/postgres.md) |
| `mssql` | Microsoft SQL Server | *(coming soon)* |
| `snowflake` | Snowflake | *(coming soon)* |

Example:

```yaml
providers:
  warehouse:
    kind: bigquery
    project: my-gcp-project
    dataset: raw_data
    location: US
    max_concurrency: 15
```

## providers.catalog

Controls the dataset catalog system that indexes table metadata for the agent.

| Field | YAML path | Default | Description |
|---|---|---|---|
| `enabled` | `providers.catalog.enabled` | `true` | Enable catalog discovery and indexing |
| `refresh_secs` | `providers.catalog.refresh_secs` | `60` | Seconds between catalog refreshes |
| `max_concurrency` | `providers.catalog.max_concurrency` | `8` | Max concurrent metadata queries |

```yaml
providers:
  catalog:
    enabled: true
    refresh_secs: 3600
    max_concurrency: 8
```

## providers.dbt

Configures the dbt integration for model authoring, validation, and publishing.

| Field | YAML path | Env var | Default | Description |
|---|---|---|---|---|
| `enabled` | `providers.dbt.enabled` | | `true` | Enable dbt provider |
| `runner` | `providers.dbt.runner` | `DBT_RUNNER` | `host` | `host` (shelled out) or `docker` |
| `target` | `providers.dbt.target` | `DBT_TARGET` | *(empty)* | dbt target (e.g. `athena`, `bigquery`, `postgres`) |
| `profiles_dir` | `providers.dbt.profiles_dir` | `DBT_PROFILES_DIR` | *(none)* | Custom dbt profiles directory |

### dbt naming

Controls how dbt materialises schemas. The naming convention uses a base schema with tier suffixes:

| Field | YAML path | Env var | Default | Description |
|---|---|---|---|---|
| `target_schema` | `providers.dbt.naming.target_schema` | `DBT_TARGET_SCHEMA` | *(empty)* | Base schema name |
| `silver_suffix` | `providers.dbt.naming.silver_suffix` | `DBT_SILVER_SUFFIX` | `silver` | Suffix for silver/staging tier |
| `gold_suffix` | `providers.dbt.naming.gold_suffix` | `DBT_GOLD_SUFFIX` | `warehouse` | Suffix for gold/mart tier |

With `target_schema: analytics`, dbt materialises to `analytics_silver` and `analytics_warehouse`.

```yaml
providers:
  dbt:
    enabled: true
    runner: host
    target: bigquery
    naming:
      target_schema: analytics
      silver_suffix: silver
      gold_suffix: gold
```

### Docker runner

When `runner: docker`, the dbt runner executes inside a Docker container:

| Field | YAML path | Env var | Default | Description |
|---|---|---|---|---|
| `docker_image` | `providers.dbt.docker_image` | `DBT_DOCKER_IMAGE` | *(none)* | Docker image reference |
| `docker_platform` | `providers.dbt.docker_platform` | `DBT_DOCKER_PLATFORM` | *(none)* | Platform (e.g. `linux/amd64`) |
| `docker_network` | `providers.dbt.docker_network` | `DBT_DOCKER_NETWORK` | *(none)* | Docker network |
| `docker_mount_aws_dir` | `providers.dbt.docker_mount_aws_dir` | `DBT_DOCKER_MOUNT_AWS_DIR` | `false` | Mount `~/.aws` into container |

## providers.vector

Toggles the LanceDB vector store.

| Field | YAML path | Default | Description |
|---|---|---|---|
| `enabled` | `providers.vector.enabled` | `true` | Enable vector store |

```yaml
providers:
  vector:
    enabled: true
```

When enabled, the vector store is initialised at the keyspace path `{scope}/lancedb` (local) or `s3://{bucket}/{scope}/lancedb` (S3).

See [LanceDB connector](../connectors/vector/lancedb.md) for details.
