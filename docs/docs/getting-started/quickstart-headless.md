# Headless Mode

ReAct can run without a WebSocket connection using `react run`. This is useful for batch workflows, CI pipelines, and automated testing.

## Basic usage

```bash
cargo run -p react -- run \
  --config my-config.yml \
  --suite-id data_engineer \
  --agent agent
```

The agent runs to completion and exits. Progress is printed to stdout.

## Flags

| Flag | Required | Default | Description |
|---|---|---|---|
| `--config` | Yes | | Path to YAML config file. Can be specified multiple times for parallel runs. |
| `--suite-id` | No | `data_engineer` | Suite to run |
| `--agent` | No | suite default | Agent type (`ask`, `agent`, `kb`, `review`) |
| `--thread-id` | No | auto-generated | Resume an existing thread |
| `--parallel` | No | off | Run multiple configs concurrently |
| `--log` | No | `info` | Log level |

## Resuming a thread

Pass `--thread-id` to continue a previously interrupted thread:

```bash
cargo run -p react -- run \
  --config my-config.yml \
  --suite-id data_engineer \
  --thread-id 7c19291d-2218-4d51-adfe-901e9fd30835
```

## Parallel execution

Run multiple configs concurrently:

```bash
cargo run -p react -- run --parallel \
  --config project-a.yml \
  --config project-b.yml \
  --suite-id data_engineer \
  --agent agent
```

Each config runs in its own task with prefixed log output. Parallel mode is non-interactive; `--terminal` is not supported.

## Auto-answering prompts

When the agent hits an `await_user` or `await_approval` interrupt, headless mode needs a way to proceed. Set `REACT_HEADLESS=true` to auto-answer prompts:

```bash
export REACT_HEADLESS=true
cargo run -p react -- run --config my-config.yml
```

## Plain progress output

For CI environments without TTY support, use plain log output:

```bash
export REACT_PLAIN_PROGRESS=true
cargo run -p react -- run --config my-config.yml --log info
```

## Where output goes

- **Thread JSON**: `{storage.path}/{tenant}/{workspace}/{project_id}/threads/{thread_id}.json`
- **Run logs**: `{storage.path}/{tenant}/{workspace}/{project_id}/logs/{thread_id}.log`
- **dbt artifacts**: `{storage.path}/{tenant}/{workspace}/{project_id}/dbt/`

## Next steps

- [CLI Reference: run](../cli/run.md) — complete flag reference
- [Configuration Overview](../configuration/overview.md) — YAML config format and precedence
