# react run

Run the agent in headless mode (no WebSocket server). Executes a single workflow and exits.

## Usage

```bash
cargo run -p react -- run --config <path> [flags]
```

## Flags

| Flag | Required | Default | Description |
|---|---|---|---|
| `--config` | Yes | | Path to YAML config file. Can be specified multiple times for parallel runs. |
| `--suite-id` | No | `data_engineer` | Suite to run |
| `--agent` | No | suite default | Agent type (`ask`, `agent`, `kb`, `review`) |
| `--thread-id` | No | auto-generated | Resume an existing thread by ID |
| `--parallel` | No | off | Run multiple `--config` files concurrently |
| `--storage-mode` | No | `local` (or config value) | Storage backend: `local` or `s3` |
| `--bucket` | No | | S3 bucket name |
| `--log` | No | `info` | Log level |

## What it does

1. Loads the YAML config file(s)
2. Initialises providers
3. Creates or resumes a thread
4. Runs the suite's agent loop to completion
5. Persists the thread and exits

In parallel mode, each config runs in its own async task with prefixed log output.

## Example

Single run:

```bash
export LLM_API_KEY="sk-..."
cargo run -p react -- run \
  --config my-config.yml \
  --suite-id data_engineer \
  --agent agent \
  --log info
```

Parallel multi-config:

```bash
cargo run -p react -- run --parallel \
  --config project-a.yml \
  --config project-b.yml \
  --suite-id data_engineer
```

Resume a thread:

```bash
cargo run -p react -- run \
  --config my-config.yml \
  --thread-id 7c19291d-2218-4d51-adfe-901e9fd30835
```

## Environment variables

| Variable | Description |
|---|---|
| `REACT_HEADLESS` | Set to `true` to auto-answer `await_user` and `await_approval` prompts |
| `REACT_PLAIN_PROGRESS` | Set to `true` for plain progress output (no terminal UI) |

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Agent completed successfully |
| Non-zero | Runtime error or agent failure |

## Notes

- Parallel mode is non-interactive. `--terminal` is not supported.
- On Windows, use `--log info` for plain output if PowerShell does not support the terminal UI.
- If the agent hits an `await_user` or `await_approval` interrupt without `REACT_HEADLESS=true`, it will block waiting for input that will never arrive.
