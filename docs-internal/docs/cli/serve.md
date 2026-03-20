# react serve

Start the WebSocket server.

## Usage

```bash
cargo run -p react -- serve --config <path> [flags]
```

## Flags

| Flag | Required | Default | Description |
|---|---|---|---|
| `--config` | Yes | | Path to YAML config file |
| `--port` | No | `8787` (or config value) | WebSocket server port |
| `--storage-mode` | No | `local` (or config value) | Storage backend: `local` or `s3` |
| `--bucket` | No | | S3 bucket name (required when `--storage-mode s3`) |
| `--storage-path` | No | `./.react` (or config value) | Local storage directory path |
| `--tenant` | No | `default` (or config value) | Tenant identifier |
| `--workspace` | No | `default` (or config value) | Workspace identifier |
| `--project-id` | No | `default` (or config value) | Project identifier |
| `--terminal` | No | off | Enable terminal UI (progress bars, interactive display) |
| `--log` | No | | Log level (`debug`, `info`, `warn`, `error`) |

## What it does

1. Loads the YAML config file and resolves configuration using the [precedence chain](../configuration/overview.md) (CLI flag > env var > YAML > default)
2. Initialises providers based on config: warehouse, catalog, dbt, vector, storage
3. Builds the suite registry (Data Engineer, KB)
4. Starts a TCP listener on the configured port
5. Accepts WebSocket upgrade requests and handles JSON frame exchange

The server runs indefinitely until terminated.

## Example

```bash
export LLM_API_KEY="sk-..."
cargo run -p react -- serve \
  --config src/runtime/config.example.yml \
  --port 8787 \
  --terminal
```

The server starts at `ws://localhost:8787/`. Connect with any WebSocket client to begin interacting.

## Notes

- By default, storage is local and persists under `storage.path` (default `./.react`)
- For S3 storage, pass `--storage-mode s3 --bucket <BUCKET>` or configure in YAML
- The `--terminal` flag enables a rich terminal UI. Omit it for plain log output (recommended for CI/production)
- Secrets (API keys, credentials) remain env-driven. See [Environment Variables](../configuration/environment-variables.md)
