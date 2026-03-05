# Flow Frames

**FlowFrames** are the output types that suites return to the WebSocket server. The server translates each `FlowFrame` into a corresponding WebSocket response frame sent to the client.

## FlowFrame types

| Type | Server frame | Description |
|---|---|---|
| `Final` | `final` | The agent produced a validated result |
| `Review` | `review` | Read-only review output (e.g. red-team analysis) |
| `AwaitUser` | `await_user` | The agent needs free-form input from the user |
| `AwaitApproval` | `await_approval` | The agent needs a binary approve/reject decision |

## Final

A `Final` frame contains a structured result:

| Field | Type | Description |
|---|---|---|
| `kind` | string | Result type discriminator (`ask`, `kb`, `generic`) |
| `payload` | object | Kind-specific payload (see below) |
| `display` | string (optional) | Human-readable display text |

### Final kinds

**ask** — answer to a data question

```json
{
  "kind": "ask",
  "payload": {
    "answer": "Rides trended up 8% vs the prior week.",
    "sql": "SELECT date, COUNT(*) AS rides FROM trips WHERE ...",
    "data": { "header": ["date", "rides"], "rows": [["2025-03-01", "1234"]] },
    "chart": { "type": "bar", "x": "date", "y": ["rides"] }
  }
}
```

**kb** — knowledge-base answer

```json
{
  "kind": "kb",
  "payload": {
    "answer": "The retention policy requires 90-day archival."
  }
}
```

**generic** — free-text result for other suite types

```json
{
  "kind": "generic",
  "payload": {
    "text": "All staging models have been validated and published."
  }
}
```

## Review

A `Review` frame contains read-only output from a reviewer agent:

| Field | Type | Description |
|---|---|---|
| `text` | string | Review commentary |
| `meta` | object (optional) | Structured review metadata (decision, tier, dataset IDs, file references) |

Review decisions: `proceed` (no issues found) or `patch_impl` (implementation changes needed).

## AwaitUser

Pauses the agent loop and prompts the user for free-form input.

| Field | Type | Description |
|---|---|---|
| `prompt` | string | Question or instruction for the user |

The client responds with a `user` frame containing the text.

## AwaitApproval

Pauses the agent loop and prompts the user for a binary decision.

| Field | Type | Description |
|---|---|---|
| `prompt` | string | Description of what needs approval |

The client responds with `approve` or `reject`.

## How suites produce FlowFrames

A suite's `handle_new`, `handle_open`, and `handle_user` methods return `Vec<FlowFrame>`. The WebSocket server iterates the list and sends each as a typed response frame. A typical sequence:

1. Suite runs preflight → emits `phase` events (not FlowFrames; these are streamed separately)
2. Suite runs agent loop → emits `tool_start`, `tool_end`, `llm_start`, `llm_end` events
3. Loop produces outcome → suite wraps it as `FlowFrame::Final`, `FlowFrame::AwaitUser`, etc.
4. Server translates and sends

## Next steps

- [WebSocket API: Server Frames](../websocket-api/server-frames.md) — the wire format for each frame type
- [WebSocket API: Schemas](../websocket-api/schemas.md) — detailed payload schemas
- [Agent Policy](agent-policy.md) — how policies control final validation and interrupts
