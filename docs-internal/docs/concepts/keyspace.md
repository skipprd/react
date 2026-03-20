# Keyspace

The **keyspace** defines the storage key layout for all persisted artifacts. Every key is scoped by `tenant / workspace / project_id`, ensuring isolation between workloads on shared infrastructure.

## Scope

Three fields form the scope:

| Field | Default | Description |
|---|---|---|
| `tenant` | `default` | Top-level tenant identifier |
| `workspace` | `default` | Logical grouping (e.g. `dev`, `prod`, `staging`) |
| `project_id` | `default` | Project or workflow identifier |

Scope values must not contain path separators (`/`, `\`) or `..`.

## Key layout

All keys follow the pattern `{tenant}/{workspace}/{project_id}/{resource_type}/...`:

| Resource | Key pattern | Description |
|---|---|---|
| **Threads** | `{scope}/threads/{thread_id}.json` | Thread step log (full conversation) |
| **Thread state** | `{scope}/state/{thread_id}/state.json` | Materialized thread state snapshot |
| **Logs** | `{scope}/logs/{thread_id}.log` | Per-thread run log |
| **Catalog** | `{scope}/catalog/{dataset_id}.yaml` | Dataset catalog entries |
| **dbt project** | `{scope}/dbt/...` | dbt project files (models, schema, project YAML) |
| **LanceDB** | `{scope}/lancedb` (local) or `s3://{bucket}/{scope}/lancedb` (S3) | Vector index data |

## Example

With scope `tenant=acme`, `workspace=prod`, `project_id=analytics`:

```
acme/prod/analytics/threads/7c19291d-2218-4d51-adfe-901e9fd30835.json
acme/prod/analytics/state/7c19291d-2218-4d51-adfe-901e9fd30835/state.json
acme/prod/analytics/logs/7c19291d-2218-4d51-adfe-901e9fd30835.log
acme/prod/analytics/catalog/orders.yaml
acme/prod/analytics/dbt/models/staging/stg_orders.sql
acme/prod/analytics/lancedb/
```

## Storage backends

The keyspace layout is independent of the storage backend. Keys resolve to:

- **Local storage** — file paths relative to `storage.path` (default `./.react`)
- **S3 storage** — object keys within `storage.bucket`

For example, the thread key `acme/prod/analytics/threads/abc.json` becomes:

- Local: `./.react/acme/prod/analytics/threads/abc.json`
- S3: `s3://my-bucket/acme/prod/analytics/threads/abc.json`

## The Keyspace trait

The `Keyspace` trait (`src/core/src/keyspace.rs`) builds keys for each resource type given a scope. Suites and tools use it to resolve storage paths without hardcoding layout conventions.

## Next steps

- [Configuration: Storage](../configuration/storage.md) — configuring local vs S3 storage
- [Threads](threads.md) — how thread data is persisted
- [Connectors: S3](../connectors/storage/s3.md) — S3 storage adapter details
