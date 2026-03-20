# Server Frames

All frames sent from the server to the client. Every frame includes `v`, `type`, `server_time`, and `seq`.

## thread_assigned

Sent in response to `new`. Contains the server-generated thread ID.

| Field | Type | Description |
|---|---|---|
| `cid` | string | Correlation ID from the `new` request |
| `thread_id` | string | Assigned thread ID |

```json
{
  "v": 1,
  "type": "thread_assigned",
  "server_time": "2025-11-16T12:00:01Z",
  "seq": 102,
  "cid": "a1111111-2222-3333-4444-555555555555",
  "thread_id": "th_1731761234567"
}
```

## final

Agent produced a validated result.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `thread_seq` | integer | Thread sequence number |
| `result` | object | Typed `FinalResult` (discriminated by `kind`) |

```json
{
  "v": 1,
  "type": "final",
  "server_time": "2025-11-16T12:00:03Z",
  "seq": 105,
  "thread_id": "th_1731761234567",
  "thread_seq": 1,
  "result": {
    "kind": "ask",
    "payload": {
      "answer": "Rides trended up 8% vs the prior week.",
      "sql": "SELECT date, COUNT(*) AS rides FROM trips WHERE ..."
    }
  }
}
```

See [Schemas](schemas.md) for the full `FinalResult` type definitions.

## review

Read-only reviewer output.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `thread_seq` | integer | Thread sequence number |
| `text` | string | Review commentary |
| `meta` | object (optional) | `ReviewDecisionMeta` with decision, tier, dataset IDs |

## await_user

Agent needs free-form input from the user.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `thread_seq` | integer | Thread sequence number |
| `prompt` | string | Question for the user |

```json
{
  "v": 1,
  "type": "await_user",
  "server_time": "2025-11-16T12:00:04Z",
  "seq": 106,
  "thread_id": "th_1731761234567",
  "thread_seq": 2,
  "prompt": "Please tell me a bit about your business (1–3 sentences)."
}
```

Respond with a `user` frame.

## await_approval

Agent requests a binary approve/reject decision.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `thread_seq` | integer | Thread sequence number |
| `prompt` | string | Description of what needs approval |

```json
{
  "v": 1,
  "type": "await_approval",
  "server_time": "2025-11-16T12:00:05Z",
  "seq": 107,
  "thread_id": "th_1731761234567",
  "thread_seq": 3,
  "prompt": "Approve the staging model for orders?"
}
```

Respond with `approve` or `reject`.

## phase

Phase transition event.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `step_idx` | integer | Index in the thread step log |
| `phase` | string | New phase name |
| `from_phase` | string (optional) | Previous phase name |
| `reason_code` | string (optional) | Suite-defined transition reason |
| `ts` | string | RFC 3339 timestamp |
| `runs` | array | Start/end windows for this phase |
| `total_runtime_ms` | integer | Cumulative runtime for this phase |

## tool_start

Tool execution started.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `tool_id` | string | Unique tool invocation ID |
| `name` | string | Tool name (e.g. `sql_run`, `dbt_validate`) |
| `clean_name` | string | Human-readable label (e.g. "Run SQL query") |
| `phase` | string (optional) | Current phase |
| `status` | string | `running` |
| `payload` | object (optional) | Tool-specific metadata |
| `ctx` | object (optional) | Execution context (plan kind, task ID, etc.) |

## tool_end

Tool execution completed.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `tool_id` | string | Tool invocation ID (matches `tool_start`) |
| `name` | string | Tool name |
| `clean_name` | string | Human-readable label |
| `status` | string | `ok` or `failed` |
| `payload` | object (optional) | Tool-specific result metadata |
| `error` | string (optional) | Error message if `failed` |
| `ctx` | object (optional) | Execution context |

## llm_start

LLM call started.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `call_id` | integer | LLM call counter |
| `phase` | string | Current phase |
| `model` | string (optional) | Model name |
| `ctx` | object (optional) | Execution context |

## llm_end

LLM call completed.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `call_id` | integer | LLM call counter (matches `llm_start`) |
| `phase` | string | Current phase |
| `model` | string (optional) | Model name |
| `status` | string | `ok` or `failed` |
| `error` | string (optional) | Error message if `failed` |
| `ctx` | object (optional) | Execution context |

## plans

Current execution plans for a thread.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `plans` | array | Array of `PlanSnapshot` objects |

## plans_changed

Notification that plans have been updated.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `changed` | array | Plan kinds that changed |
| `changed_plan_keys` | array (optional) | Specific plan keys that changed |

Clients should fetch `plans` after receiving this frame to get the latest snapshots.

## thread_state

Materialised thread state snapshot.

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread ID |
| `state` | object | `ThreadStateSnapshot` |

## list

Response to `list` request. Contains thread summaries.

## suites

Response to `suites` request. Contains available suites and their allowed agent types.

## history

Response to `history` request. Contains paginated messages.

## unread

Unread message count for a thread.

## ok

Acknowledgement for operations like `delete` and `seen`.

## error

Error response with message, code, and optional `retry_after_ms`.
