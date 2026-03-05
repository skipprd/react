# Storage

ReAct supports two storage backends: local filesystem (default) and Amazon S3.

## storage.mode

| | |
|---|---|
| **YAML path** | `storage.mode` |
| **CLI flag** | `--storage-mode` |
| **Env var** | `REACT_STORAGE_MODE` |
| **Default** | `local` |
| **Values** | `local`, `s3` |

## storage.path

Local storage directory. Only used when `mode` is `local`.

| | |
|---|---|
| **YAML path** | `storage.path` |
| **CLI flag** | `--storage-path` |
| **Env var** | `REACT_STORAGE_PATH` |
| **Default** | `./.react` |

Relative paths are resolved against the current working directory and stored as absolute paths internally.

## storage.bucket

S3 bucket name. Required when `mode` is `s3`.

| | |
|---|---|
| **YAML path** | `storage.bucket` |
| **CLI flag** | `--bucket` |
| **Env var** | `SKIPPR_S3_BUCKET` |
| **Default** | *(none — required for S3 mode)* |

## Local storage example

```yaml
storage:
  mode: local
  path: ./.react
```

Files are written under the `path` directory following the [Keyspace](../concepts/keyspace.md) layout:

```
./.react/acme/prod/analytics/threads/abc.json
./.react/acme/prod/analytics/catalog/orders.yaml
```

## S3 storage example

```yaml
storage:
  mode: s3
  bucket: my-react-bucket
```

Objects are written to the S3 bucket using the same keyspace layout:

```
s3://my-react-bucket/acme/prod/analytics/threads/abc.json
```

S3 storage uses the standard AWS credential chain (environment variables, instance profile, or IAM role). See [Connectors: S3](../connectors/storage/s3.md).
