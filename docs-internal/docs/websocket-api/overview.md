# WebSocket API Overview

ReAct communicates over a single WebSocket endpoint. Clients send JSON frames and receive JSON frames. The API is defined in OpenAPI 3.1 at `src/runtime/openapi/ws-core.yaml`.

## Connection

```
ws://{host}:{port}/
```

Default: `ws://localhost:8787/`

Connect via a standard WebSocket upgrade. No authentication is enforced by default — do not expose publicly without adding an auth layer.

## Frame format

Every frame is a JSON object with at minimum:

| Field | Type | Description |
|---|---|---|
| `v` | integer | Protocol version (currently `1`) |
| `type` | string | Frame type discriminator |

Client frames additionally include:

| Field | Type | Description |
|---|---|---|
| `cid` | string | Client correlation ID (UUID). Used to match responses to requests. |

Server frames additionally include:

| Field | Type | Description |
|---|---|---|
| `server_time` | string | RFC 3339 timestamp |
| `seq` | integer | Monotonically increasing sequence number |

Thread-scoped server frames also include:

| Field | Type | Description |
|---|---|---|
| `thread_id` | string | Thread identifier |
| `thread_seq` | integer | Thread-scoped sequence number |
| `for_cid` | string (optional) | Correlation ID of the client request that triggered this frame |

## Message flow

### New thread

```
Client                          Server
  │                               │
  ├── new ──────────────────────▶ │
  │                               ├── thread_assigned
  │                               ├── phase (preflight)
  │                               ├── tool_start
  │                               ├── tool_end
  │                               ├── llm_start
  │                               ├── llm_end
  │                               ├── phase (plan)
  │                               │   ... more events ...
  │                               ├── final
  │                               │
```

### Interrupt flow

```
Client                          Server
  │                               │
  │                               ├── await_user
  ├── user ─────────────────────▶ │
  │                               ├── tool_start
  │                               │   ... agent resumes ...
  │                               ├── final
  │                               │
```

### Approval flow

```
Client                          Server
  │                               │
  │                               ├── await_approval
  ├── approve ──────────────────▶ │
  │                               ├── tool_start
  │                               │   ... agent resumes ...
  │                               ├── final
  │                               │
```

## Error handling

Errors are returned as `error` frames:

```json
{
  "v": 1,
  "type": "error",
  "server_time": "2025-11-16T12:00:00Z",
  "error": "suite 'unknown_suite' not found",
  "code": "not_found",
  "cid": "..."
}
```

Error codes:

| Code | Description |
|---|---|
| `invalid_request` | Malformed frame or missing required fields |
| `not_found` | Thread or suite not found |
| `thread_closed` | Thread is no longer accepting messages |
| `rate_limited` | Too many requests (includes `retry_after_ms`) |
| `internal` | Server error |

## Next steps

- [Client Frames](client-frames.md) — all client-to-server frame types
- [Server Frames](server-frames.md) — all server-to-client frame types
- [Schemas](schemas.md) — detailed payload schemas
