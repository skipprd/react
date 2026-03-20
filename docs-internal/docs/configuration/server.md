# Server

## server.port

WebSocket server listen port.

| | |
|---|---|
| **YAML path** | `server.port` |
| **CLI flag** | `--port` |
| **Default** | `8787` |

## scope.tenant

Top-level tenant identifier. Used as the first segment of all storage keys.

| | |
|---|---|
| **YAML path** | `scope.tenant` |
| **CLI flag** | `--tenant` |
| **Default** | `default` |

Must not contain path separators (`/`, `\`) or `..`.

## scope.workspace

Logical grouping for pipelines, typically an environment or domain.

| | |
|---|---|
| **YAML path** | `scope.workspace` |
| **CLI flag** | `--workspace` |
| **Default** | `default` |

## scope.project_id

Project or workflow identifier.

| | |
|---|---|
| **YAML path** | `scope.project_id` |
| **CLI flag** | `--project-id` |
| **Default** | `default` |

## Example

```yaml
server:
  port: 9090

scope:
  tenant: acme
  workspace: prod
  project_id: analytics
```

All storage keys will be prefixed with `acme/prod/analytics/`. See [Keyspace](../concepts/keyspace.md).
