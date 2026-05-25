# Chat agent context (react transport)

## Implementation

- Headless thread resume: when `headless_prompt` is a real user message (not `go` / `continue`), the runner sends a `user` frame instead of `open`, matching WebSocket semantics for follow-up turns.

Consumed by `skipprd` via path dependency on `react` 1.3.0 (`HeadlessRunOpts.headless_prompt`, `stream_jsonl`, `HeadlessRunOutcome.thread_id`).

## Human review

Validate via `skippr chat send --output jsonl` multi-turn smoke tests from the Skippr IDE or CLI after rebuilding `skipprd` against this `react` revision.
