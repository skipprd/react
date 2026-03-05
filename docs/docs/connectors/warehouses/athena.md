# Athena

The Athena provider uses AWS Athena for SQL execution and AWS Glue Data Catalog for table/schema discovery. This is the `provider-athena` module (`src/modules/provider-athena/`).

## Configuration

```yaml
providers:
  warehouse:
    kind: athena
    workgroup: my_workgroup
    catalog: AwsDataCatalog
    schema: raw_data
    region: us-east-1
    result_s3: s3://my-query-results/
    max_concurrency: 15
    discovery_cache_ttl_secs: 120
```

| Field | YAML path | Default | Description |
|---|---|---|---|
| `workgroup` | `providers.warehouse.workgroup` | *(none)* | Athena workgroup for query execution |
| `catalog` | `providers.warehouse.catalog` | `AwsDataCatalog` | Glue Data Catalog name |
| `schema` | `providers.warehouse.schema` | *(empty)* | Default Glue database for discovery and unqualified queries |
| `region` | `providers.warehouse.region` | *(none)* | AWS region override |
| `result_s3` | `providers.warehouse.result_s3` | *(none)* | S3 location for Athena query results |
| `max_concurrency` | `providers.warehouse.max_concurrency` | *(none)* | Max concurrent Athena queries |
| `discovery_cache_ttl_secs` | `providers.warehouse.discovery_cache_ttl_secs` | *(none)* | TTL for cached discovery results (seconds) |

## Authentication

Uses the standard AWS credential chain:

1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
2. Instance profile / IAM role
3. AWS SSO / credential file

Set the region:

```bash
export AWS_DEFAULT_REGION=us-east-1
```

## AWS permissions

The IAM identity running ReAct needs:

| Permission | Purpose |
|---|---|
| `athena:StartQueryExecution` | Execute SQL queries |
| `athena:GetQueryExecution` | Poll query status |
| `athena:GetQueryResults` | Fetch query results |
| `s3:PutObject`, `s3:GetObject` on results bucket | Athena writes/reads results |
| `glue:GetDatabase`, `glue:GetDatabases` | Database discovery |
| `glue:GetTable`, `glue:GetTables` | Table discovery |
| `glue:GetPartitions` | Partition metadata |

## dbt target

When using dbt with Athena, set:

```yaml
providers:
  dbt:
    target: athena
```

And install the dbt adapter:

```bash
pip install dbt-athena-community
```

## Type mapping

The Athena provider maps Athena/Glue types to ReAct's internal type system. This mapping is handled by the `type_parse` module and supports standard Athena types (string, int, bigint, double, boolean, timestamp, date, array, struct, map).
