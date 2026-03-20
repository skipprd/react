# Threads

A **thread** is a conversation between a client and the agent. It contains all user messages, tool calls, observations, phase transitions, and final results. Threads are persisted as JSON and can be resumed across connections and process restarts.

## Thread lifecycle

```
new ──▶ thread_assigned ──▶ agent loop ──▶ final
                                │
                                ├──▶ await_user ──▶ user ──▶ agent loop
                                └──▶ await_approval ──▶ approve/reject ──▶ agent loop
```

1. **Create** — client sends a `new` frame with `suiteId`, `agentType`, and an optional `question`
2. **Assign** — server generates a thread ID and responds with `thread_assigned`
3. **Execute** — the suite runs the agent loop, streaming progress events
4. **Pause** — the loop may pause for user input (`await_user`) or approval (`await_approval`)
5. **Resume** — client responds with `user`, `approve`, or `reject` to continue
6. **Complete** — the loop produces a `final` frame with the result
7. **Reopen** — client sends `open` with an existing `thread_id` to resume later

## Thread steps

Each action in a thread is recorded as a step. Step types:

| Step type | Description |
|---|---|
| `user` | User message text |
| `phase` | Phase transition (e.g. preflight → plan → author) |
| `tool_start` | Tool execution started (name, args) |
| `tool_end` | Tool execution completed (observation, status, runtime) |
| `llm_start` | LLM call started |
| `llm_end` | LLM call completed |
| `switch_suite` | Suite changed mid-thread |
| `switch_agent` | Agent type changed mid-thread |
| `final` | Final result produced (kind, payload, display) |

## Persistence

Threads are stored as JSON files via the `StorageAdapter` at keys determined by the `Keyspace`:

```
{tenant}/{workspace}/{project_id}/threads/{thread_id}.json
```

The `ThreadStore` handles reading and writing thread steps. It uses the `StorageAdapter` (local file system or S3) so persistence is transparent to suites and the agent loop.

## State hierarchy

Thread state is split into three tiers:

### 1. ControlState (primary — source of truth)

```
{tenant}/{workspace}/{project_id}/state/{thread_id}/control.json
```

`ControlStateStore` holds the authoritative execution state: current phase, progress, suite_id. Suites own and mutate this exclusively. The system can function correctly with only this file intact.

### 2. ThreadLog (secondary — audit log)

The append-only step log described above. Read access is restricted to the `ThreadLogReader` trait (compile-time read-only guarantee). Write access is append-only via `ThreadLogWriter`.

### 3. ThreadLogViewCache (tertiary — display projection)

```
{tenant}/{workspace}/{project_id}/state/{thread_id}/state.json
```

A disposable, materialized projection of the ThreadLog for display purposes. Contains:

- Current suite and agent type (for display)
- Current phase (materialized from log)
- Per-item status tracking (for multi-item plans)
- Total runtime across phases
- Suite-owned display state (plan summaries, etc.)

This cache can be fully rebuilt from the ThreadLog at any time. Materialization is non-fatal — if it fails, the log append still succeeds.

## Schema versioning

Thread JSON files include a `schema_version` field (currently version 4). The view cache snapshot has its own version (currently version 2). The control state envelope has its own version (currently version 1). Version checks enforce strict compatibility with no migration support.

## History and pagination

Clients can request thread history via the `history` frame:

```json
{
  "v": 1,
  "type": "history",
  "cid": "...",
  "thread_id": "7c19291d-2218-4d51-adfe-901e9fd30835",
  "limit": 50
}
```

The server returns messages with `thread_seq` ordering. Use `before_thread_seq` for backward pagination.

## Deleting threads

The `delete` frame removes a thread's JSON file and any thread-scoped embeddings from the vector store.

## Next steps

- [Keyspace](keyspace.md) — how persistence keys are constructed
- [Flow Frames](flow-frames.md) — the output types the server sends back
- [WebSocket API: Client Frames](../websocket-api/client-frames.md) — `new`, `open`, `user`, `history`, `delete`
