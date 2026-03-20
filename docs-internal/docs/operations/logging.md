# Logging

ReAct uses structured logging via the standard Rust `tracing` ecosystem. Logs are written to stdout and optionally to per-thread log files.

## Log levels

Control log verbosity via:

- CLI: `--log info` (or `debug`, `warn`, `error`)
- Env: `RUST_LOG=info` (supports module-level filters like `RUST_LOG=react=debug,warn`)

## Log output

| Output | Location | Description |
|---|---|---|
| **stdout** | Terminal | Real-time log stream (when `--terminal` is off or `REACT_PLAIN_PROGRESS=true`) |
| **Per-thread log** | `{scope}/logs/{thread_id}.log` | Detailed log for each agent run |
| **Rotating log** | `{REACT_LOG_DIR}/react.log` | Daily-rotating log file (when `REACT_LOG_DIR` is set) |

## Event map

Use this as a quick lookup: **event → meaning → action**.

### Progress events

| Log event | Meaning | Action |
|---|---|---|
| `Starting WebSocket server on port ...` | Server startup | None |
| `Suite registry: [data_engineer, kb]` | Suites loaded | Verify expected suites are present |
| `Thread created: ...` | New thread started | None |
| `Phase transition: ... → ...` | Agent moved to next phase | None |
| `Tool call: ... (args=...)` | Tool execution started | None |
| `Tool result: ... (status=..., ms=...)` | Tool execution completed | Watch for `failed` status |
| `LLM call #... (model=..., tokens=...)` | LLM request sent | None |
| `LLM response #... (status=..., ms=...)` | LLM response received | Watch for `failed` status |
| `Final accepted (kind=...)` | Agent produced a validated result | None |
| `Thread persisted: ...` | Thread saved to storage | None |

### Warning events

| Log event | Meaning | Action |
|---|---|---|
| `Tool timeout: ... exceeded ...s` | Tool exceeded its timeout | Check tool health or increase timeout |
| `LLM parse error: ...` | LLM produced invalid JSON | Fed back as error observation; agent self-corrects |
| `Final rejected by policy: ...` | Policy rejected the final | Agent continues with feedback |
| `Step budget exhausted (max_steps=...)` | Agent hit step limit | Policy fallback triggered |
| `Retry budget exhausted for ...` | dbt repair retries exhausted | Manual intervention may be needed |

### Error events

| Log event | Meaning | Action |
|---|---|---|
| `Provider error: ...` | Warehouse/storage operation failed | Check credentials and connectivity |
| `LLM request failed: ...` | LLM API call failed (401/429/timeout) | Check API key, rate limits, timeout settings |
| `Storage error: ...` | Read/write to storage failed | Check storage config and permissions |
| `dbt validation failed: ...` | dbt build/run returned errors | Check dbt logs; repair may be attempted automatically |

## Terminal UI

When `--terminal` is passed to `react serve`, a rich terminal UI is displayed with:

- Phase progress bars
- Tool execution timeline
- LLM call indicators

Disable with `REACT_PLAIN_PROGRESS=true` for CI environments or non-TTY terminals.
