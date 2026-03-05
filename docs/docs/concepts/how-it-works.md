# How It Works

ReAct is a single Rust binary that runs an agent loop over WebSocket. The loop follows the ReAct pattern: the LLM reasons about a task, calls tools, observes results, and repeats until it produces a final answer or hits an interrupt.

## Architecture overview

```
Client (WS / CLI)
  │
  ▼
WebSocket Server
  │  parse + validate JSON frames
  │  manage thread lifecycle
  ▼
Suite Router
  │  select suite by suiteId
  ▼
Suite
  │  build tool registry
  │  inject system prompt + tool card
  │  configure agent policy
  │  run optional preflight
  ▼
ReAct Loop
  │
  ├─▶ LLM Call (system prompt + transcript)
  │       │
  │       ▼
  │   Parse JSON action
  │       │
  │   ┌───┴───────────────┐
  │   │                   │
  │   ▼                   ▼
  │  Tool action         Final action
  │   │                   │
  │   ▼                   ▼
  │  Execute tool        Policy validates
  │   │                   │
  │   ▼                   ├─▶ Accept → return result
  │  Append observation   └─▶ Reject → continue loop
  │   │
  │   ▼
  │  Policy interrupt check
  │   │
  │   ├─▶ No interrupt → next iteration
  │   └─▶ AwaitUser / AwaitApproval → pause
  │
  ▼
Thread Persistence (JSON via StorageAdapter)
```

## The ReAct loop

The core loop lives in `src/core/src/agent/run_loop.rs`. Each iteration:

1. **Build prompt** — system prompt, tool card (available tool names and descriptions), prelude lines (policy-injected context like catalog data or plan state), and the full transcript so far
2. **Call LLM** — send the prompt to the configured LLM provider and parse the response as strict JSON
3. **Parse action** — the LLM must produce one of two action types:
   - `{"type": "tool", "name": "<tool_name>", "args": {...}}` — invoke a tool
   - `{"type": "final", "final": {"kind": "...", "payload": {...}, "display": "..."}}` — produce a final result
4. **Execute** — for tool actions, the `ToolRegistry` dispatches to the named tool. The tool returns a JSON observation that is appended to the transcript
5. **Policy check** — after each tool execution, the `AgentPolicy` can convert the action into an interrupt (`AwaitUser` or `AwaitApproval`), pausing the loop until the client responds
6. **Final validation** — for final actions, the policy's `handle_final` method validates the result. It can accept (ending the loop) or reject (appending feedback to the transcript and continuing)
7. **Step budget** — if the loop reaches `max_steps` without an accepted final, the policy's `fallback` method produces a default outcome

## Key properties

- **Strict JSON actions** — the LLM must produce valid JSON matching one of the two action schemas. Parse failures are fed back as error observations so the LLM can self-correct
- **Transcript state** — the full conversation (system prompt, user messages, tool calls, observations, errors) is maintained in memory during the loop and persisted to storage as thread steps
- **Suite-agnostic core** — the loop does not know which tools exist or what "final" means. All domain logic is injected via the suite's tool registry and agent policy
- **Resumable** — threads are persisted after every step. If the process crashes or the client disconnects, the thread can be reopened and resumed from the last persisted state

## Configuration

The agent loop respects these `AgentCtx` parameters:

| Parameter | Default | Description |
|---|---|---|
| `max_steps` | suite-defined | Maximum tool-call iterations before fallback |
| `per_step_timeout_secs` | suite-defined | Timeout for each tool execution |
| `top_k` | suite-defined | Number of context items to retrieve for prelude |

## Next steps

- [Suites](suites.md) — how suites compose tools, prompts, and policies
- [Agent Policy](agent-policy.md) — how finals and interrupts are controlled
- [Threads](threads.md) — persistence and resumption
- [Flow Frames](flow-frames.md) — the output types produced by suites
