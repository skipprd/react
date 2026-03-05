# Local Storage

The local storage adapter persists all artifacts to the local filesystem. This is the default storage backend.

## Configuration

```yaml
storage:
  mode: local
  path: ./.react
```

| Field | YAML path | Env var | Default | Description |
|---|---|---|---|---|
| `mode` | `storage.mode` | `REACT_STORAGE_MODE` | `local` | Set to `local` |
| `path` | `storage.path` | `REACT_STORAGE_PATH` | `./.react` | Root directory for all storage |

Relative paths are resolved against the current working directory.

## File layout

All files follow the [Keyspace](../../concepts/keyspace.md) layout under the configured path:

```
.react/
└── {tenant}/
    └── {workspace}/
        └── {project_id}/
            ├── threads/
            │   └── {thread_id}.json
            ├── state/
            │   └── {thread_id}/
            │       └── state.json
            ├── logs/
            │   └── {thread_id}.log
            ├── catalog/
            │   └── {dataset_id}.yaml
            ├── dbt/
            │   ├── dbt_project.yml
            │   └── models/
            │       ├── staging/
            │       └── marts/
            └── lancedb/
```

## Operations

The `LocalFileStorageAdapter` implements the `StorageAdapter` trait:

| Operation | Behaviour |
|---|---|
| `get_json` | Read and parse a JSON file |
| `put_json` | Write a JSON file (creates parent directories) |
| `get_bytes` | Read raw bytes from a file |
| `put_bytes` | Write raw bytes to a file |
| `delete_object` | Delete a file |
| `head_etag` | Not supported for local files (returns `None`) |
| `list_prefix` | List files under a directory prefix |

## When to use

Local storage is ideal for:

- Local development
- Single-machine deployments
- Testing and CI pipelines
- Environments without AWS access

For production multi-machine deployments or durable storage, use [S3](s3.md).
