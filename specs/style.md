# Code style (Skippr ReAct)

Canonical code style for this repository. Locked by [decisions.md](./decisions.md) **R1–R4**. Complements [tiger-style.md](./tiger-style.md). Rust-facing agent rule: `.cursor/rules/rust-style.mdc`.

Always favour **compile errors over runtime convention**.

## 1. Design by construction

If an invariant can be enforced by the compiler, it MUST be.

```rust
// Forbidden: string modes, boolean pairs, “don’t call this wrong”
fn run(mode: &str, chat: bool, ask: bool) { /* … */ }

// Required: illegal combinations cannot be named
enum Mode { Chat, Ask, Agent, Review }
fn run(mode: Mode) { /* … */ }
```

- Newtype ids (`ThreadId`, `RunId`, `Seq`, `ToolName`) parsed **once at the edge**.
- `Option` / `Result` / enums over sentinels, `unwrap()` on library paths, or panics on valid input.
- Typed error enums at crate boundaries (`thiserror`). Avoid stringly `anyhow` in `react-core`.
- Feature gates MUST compile: `cargo check --all-features` when touching `cfg(feature)`.

## 2. One design, shallow modules

- One type, enum, CLI, and journal per job. If a parallel module exists, delete it in the same change ([hard-cutover](../.cursor/skills/hard-cutover/SKILL.md)).
- Flat functions over premature traits. Extract a trait only when there are (or will immediately be) **multiple implementors**.
- No re-export-only modules. No `_legacy` / `#[allow(dead_code)]` to keep the old world.
- Match existing formatting; run `cargo fmt` on touched packages.

## 3. Kernel vs host

| Kernel (`react-core`) | Host / adapters |
|-----------------------|-----------------|
| No `std::env`, sockets, filesystem, wall clock, OS RNG | Own real I/O behind the **same** traits the simulator implements |
| Deterministic given injected world | Product config (model, bind, storage URI) is explicit |
| Same commit path in sim and prod | MUST NOT fork a test-only kernel |

## 4. Effects and errors

- Decide under checks → ordered effects → execute with timeouts/limits → cheap postconditions.
- Durable writes: idempotency key + OCC + commit boundary ([tiger-style.md](./tiger-style.md) §5–6).
- Fail closed at trust boundaries. Silent fallback on protocol, tenancy, or budget is a bug.
- No `unwrap` / `expect` on untrusted input (LLM JSON, WS frames, UTF-8 slices, map keys).

## 5. Bounds

- Safety ceilings live in **code constants / types** (`NonZeroU32` budgets, max payload).
- Channels and buffers are **bounded**.
- Do not add impl env knobs (`FOO_BATCH_SIZE`, `WORKER_THREADS`, `CHANNEL_DEPTH`).
- Auto-tune working setpoints under those ceilings.

## 6. Naming and DRY

- Extract shared validation, serialization, and error construction. Duplicated messages are bugs waiting to diverge.
- Capability / crate names match the workspace (`react-core`, `react-transport`). Do not invent a second name for the same job.
- Comments explain **why**, not what the type already states.

## 7. Formatting and lint

```bash
cargo fmt --all -- --check
cargo clippy          # do not add -D warnings unless asked
cargo check --all-features
```

Match the crate you edit. Do not reformat unrelated modules.
