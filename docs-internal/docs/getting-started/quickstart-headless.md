# Headless Mode

ReAct can run without a WebSocket connection through app-owned hosts that call the shared headless runtime. This is useful for batch workflows, CI pipelines, and automated testing.

## Basic usage

```bash
cargo run -p skippr -- --log info run
```

The agent runs to completion and exits. Progress is printed to stdout.

## Flags

| Flag | Required | Default | Description |
|---|---|---|---|
| `--config` | Yes | | Path to YAML config file. Can be specified multiple times for parallel runs. |
| `--suite-id` | No | host default | Suite to run |
| `--agent` | No | suite default | Agent type (`ask`, `agent`, `kb`, `review`) |
| `--thread-id` | No | auto-generated | Resume an existing thread |
| `--parallel` | No | off | Run multiple configs concurrently |
| `--log` | No | `info` | Log level |

## Resuming a thread

Pass `--thread-id` to continue a previously interrupted thread:

Use the host-specific CLI for resume semantics. Host CLIs define their own resume semantics.

## Parallel execution

Run multiple configs concurrently:

Parallel multi-config execution now belongs to the host binary that owns those workflows.

Each config runs in its own task with prefixed log output. Parallel mode is non-interactive; `--terminal` is not supported.

## Auto-answering prompts

When the agent hits an `await_user` or `await_approval` interrupt, headless mode needs a way to proceed. Set `REACT_HEADLESS=true` to auto-answer prompts:

```bash
export REACT_HEADLESS=true
```

## Plain progress output

For CI environments without TTY support, use plain log output:

```bash
export REACT_PLAIN_PROGRESS=true
```

## Where output goes

- **Thread JSON**: `{storage.path}/{tenant}/{workspace}/{project_id}/threads/{thread_id}.json`
- **Run logs**: `{storage.path}/{tenant}/{workspace}/{project_id}/logs/{thread_id}.log`
- **dbt artifacts**: `{storage.path}/{tenant}/{workspace}/{project_id}/dbt/`

## Next steps

- [Headless execution](../cli/run.md) — packaging notes for the library-first runtime
- [Configuration Overview](../configuration/overview.md) — YAML config format and precedence
