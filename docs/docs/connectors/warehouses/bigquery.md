# BigQuery

The BigQuery provider uses Google BigQuery for SQL execution and dataset discovery. This is the `provider-bigquery` module (`src/modules/provider-bigquery/`).

## Configuration

```yaml
providers:
  warehouse:
    kind: bigquery
    project: my-gcp-project
    dataset: raw_data
    location: US
    max_concurrency: 15
    discovery_cache_ttl_secs: 120
```

| Field | YAML path | Default | Description |
|---|---|---|---|
| `project` | `providers.warehouse.project` | *(empty)* | Google Cloud project ID |
| `dataset` | `providers.warehouse.dataset` | *(empty)* | Default BigQuery dataset for discovery |
| `location` | `providers.warehouse.location` | *(none)* | BigQuery data location (e.g. `US`, `EU`) |
| `max_concurrency` | `providers.warehouse.max_concurrency` | *(none)* | Max concurrent BigQuery queries |
| `discovery_cache_ttl_secs` | `providers.warehouse.discovery_cache_ttl_secs` | *(none)* | TTL for cached discovery results (seconds) |

## Authentication

BigQuery supports two authentication methods:

### Application Default Credentials (recommended)

```bash
gcloud auth application-default login
```

### Service account JSON

```bash
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/service-account.json"
```

## GCP permissions

The service account or authenticated user needs:

| Role | Purpose |
|---|---|
| BigQuery Job User | Execute queries |
| BigQuery Data Viewer | Read source datasets for discovery |
| BigQuery Data Editor | Create datasets/tables for dbt materialisation |

## dbt target

When using dbt with BigQuery, set:

```yaml
providers:
  dbt:
    target: bigquery
```

And install the dbt adapter:

```bash
pip install dbt-bigquery
```

## Public datasets

BigQuery supports cross-project references. You can discover tables from public datasets by setting the project to the public dataset project (e.g. `bigquery-public-data`) and the dataset to the target dataset.

## Location mismatch

If queries fail with a location mismatch error, ensure `providers.warehouse.location` matches the dataset's actual location in BigQuery.
