---
name: tiger-style
description: >-
  Applies Skippr ReAct Tiger Style (async/multi-threaded kernel with
  exactly-once consistency, monotonics, OCC, injected world, and
  deterministic simulation). Use when implementing or refactoring
  react-core, session/store, agent loop, transport state machines,
  adapters' durable paths, or when writing specs/docs that mention
  consistency, side effects, or simulation.
---

# Tiger Style (Skippr ReAct)

Normative: [specs/tiger-style.md](../../../specs/tiger-style.md), **R1**. Always-on rule: `.cursor/rules/tiger-style.mdc`.

## Rules

1. **Name the layer first** — kernel vs host/adapters. Never blur them.
2. **Kernel** durable mutations: all-or-nothing, effectively exactly-once under retry (idempotency/dedupe + commit boundary), monotonics + OCC.
3. **Host/adapters** MAY be at-least-once (network, LLM). Kernel EOS does not upgrade product copy; host at-least-once MUST NOT weaken the kernel.
4. **Runtime:** async, event-based, multi-threaded. MUST NOT mandate process-wide single-threading.
5. **Kernel purity:** no `std::env`, FS, net, wall clock, OS RNG. Inject `Clock` / `Entropy` / `Storage` / `Llm` / `Net`. The simulator runs this code.
6. **Assert / fail-closed** on protocol, tenancy, budget; fixed safety ceilings in code.
7. **Auto-tune** pools, channels, batches, depths, memory under ceilings. MUST NOT grow impl env knobs. Product config stays explicit.
8. Hard cutover and TDD still apply (skills `hard-cutover`, `tdd-react`).

## Workflow

1. **Layer** — Kernel state machine or host I/O?
2. **Commit boundary** — What state and side effects must succeed together?
3. **Idempotency** — Key or natural key; durable dedupe/result on retry.
4. **OCC / monotonics** — `Seq`/version; conditional write; conflict → retry.
5. **Injected world** — New effects go through traits, not ambient OS.
6. **Bounds + auto-tune** — Hard ceilings in code; working setpoints auto-tuned.
7. **Asserts** — Fail closed; no silent security/protocol fallbacks.
8. **Tests** — Simulation + retry/OCC; no sleep (skill `tdd-react`).
9. **Docs** — Update `specs/` if behaviour changed.

## Anti-patterns

- Hidden `Instant::now` / `Uuid::new_v4` / `std::env` in `react-core`
- Unbounded `mpsc` / buffers “just for now”
- Last-write-wins concurrent thread-store updates without a decision
- `FOO_BATCH_SIZE` / `WORKER_THREADS` env sprawl
- A test-only fork of the run loop
- Sleep-based tests

## Related

- Skill `rust-react-crate` — Rust crate implementation
- Skill `tdd-react` — unit, functional, sim, property, fuzz
- Skill `hard-cutover` — one way, no shims
- Rule `.cursor/rules/rust-style.mdc`
