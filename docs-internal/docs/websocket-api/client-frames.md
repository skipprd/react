# Client Frames

All frames sent from the client to the server. Every frame requires `v` (protocol version), `type` (frame discriminator), and `cid` (client correlation ID).

## list

List all existing threads.

```json
{
  "v": 1,
  "type": "list",
  "cid": "b2a4c2b5-1d19-4b5c-a0b3-2f8f7a7c9d11"
}
```

**Response:** `list` frame with thread summaries.

## suites

List available suites and their allowed agent types.

```json
{
  "v": 1,
  "type": "suites",
  "cid": "c2a4c2b5-1d19-4b5c-a0b3-2f8f7a7c9d11"
}
```

**Response:** `suites` frame with suite IDs, labels, allowed agent types, and the default suite.

## new

Create a new thread and optionally start the agent with an initial question.

| Field | Required | Description |
|---|---|---|
| `suiteId` | Yes | Suite to use (e.g. `kb` or a host-provided suite) |
| `agentType` | Yes | Agent mode supported by that suite |
| `question` | No | Initial message. If absent, creates an empty thread. |

```json
{
  "v": 1,
  "type": "new",
  "cid": "a1111111-2222-3333-4444-555555555555",
  "suiteId": "kb",
  "agentType": "kb",
  "question": "Summarize the indexed docs."
}
```

**Response:** `thread_assigned` (with the new `thread_id`), then progress events, then `final` or an interrupt.

## open

Reopen an existing thread and optionally send a nudge message.

| Field | Required | Description |
|---|---|---|
| `thread_id` | Yes | Existing thread ID |
| `suiteId` | Yes | Suite to use |
| `agentType` | Yes | Agent mode (can differ from the original) |
| `question` | No | Optional nudge to resume (default: "Continue.") |

```json
{
  "v": 1,
  "type": "open",
  "cid": "...",
  "thread_id": "7c19291d-2218-4d51-adfe-901e9fd30835",
  "suiteId": "kb",
  "agentType": "kb"
}
```

**Response:** Progress events and then `final` or an interrupt. If the `agentType` differs from the current thread, a `switch_agent` step is recorded.

## user

Send user input in response to an `await_user` prompt.

| Field | Required | Description |
|---|---|---|
| `thread_id` | Yes | Thread ID |
| `text` | Yes | User-provided content |

```json
{
  "v": 1,
  "type": "user",
  "cid": "...",
  "thread_id": "7c19291d-2218-4d51-adfe-901e9fd30835",
  "text": "We rent e-bikes in NYC and care about weekend demand."
}
```

**Response:** Agent resumes with progress events.

## approve

Approve an `await_approval` request.

| Field | Required | Description |
|---|---|---|
| `thread_id` | Yes | Thread ID |

```json
{
  "v": 1,
  "type": "approve",
  "cid": "...",
  "thread_id": "7c19291d-2218-4d51-adfe-901e9fd30835"
}
```

## reject

Reject an `await_approval` request.

| Field | Required | Description |
|---|---|---|
| `thread_id` | Yes | Thread ID |

```json
{
  "v": 1,
  "type": "reject",
  "cid": "...",
  "thread_id": "7c19291d-2218-4d51-adfe-901e9fd30835"
}
```

## history

Fetch thread message history with pagination.

| Field | Required | Description |
|---|---|---|
| `thread_id` | Yes | Thread ID |
| `before_thread_seq` | No | Fetch messages before this sequence number |
| `limit` | No | Max messages to return (default 50) |

```json
{
  "v": 1,
  "type": "history",
  "cid": "...",
  "thread_id": "7c19291d-2218-4d51-adfe-901e9fd30835",
  "limit": 20
}
```

**Response:** `history` frame with messages, title, suite/agent type, and `next_before_thread_seq` for pagination.

## seen

Mark messages as read up to a sequence number.

| Field | Required | Description |
|---|---|---|
| `thread_id` | Yes | Thread ID |
| `up_to_thread_seq` | Yes | Mark all messages up to and including this sequence as read |

```json
{
  "v": 1,
  "type": "seen",
  "cid": "...",
  "thread_id": "...",
  "up_to_thread_seq": 15
}
```

## plans

Fetch current execution plans for a thread.

| Field | Required | Description |
|---|---|---|
| `thread_id` | Yes | Thread ID |

```json
{
  "v": 1,
  "type": "plans",
  "cid": "...",
  "thread_id": "..."
}
```

**Response:** `plans` frame with plan snapshots.

## thread_state

Fetch the materialised thread state snapshot.

| Field | Required | Description |
|---|---|---|
| `thread_id` | Yes | Thread ID |

```json
{
  "v": 1,
  "type": "thread_state",
  "cid": "...",
  "thread_id": "..."
}
```

**Response:** `thread_state` frame with the full state snapshot.

## delete

Delete a thread. Removes the thread JSON file and any thread-scoped embeddings.

| Field | Required | Description |
|---|---|---|
| `thread_id` | Yes | Thread ID |

```json
{
  "v": 1,
  "type": "delete",
  "cid": "...",
  "thread_id": "..."
}
```

**Response:** `ok` frame on success.
