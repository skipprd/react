# AGENTS.md — Skippr ReAct

This repository is **Skippr ReAct** — suite-neutral runtime crates consumed by product hosts such as Skippr Data Engineer (`sde`). Prefer this file and `specs/` over inventing behavior.

Public docs: [react.skippr.io](https://react.skippr.io). Markdown is `docs/`; VitePress config is `docs/.vitepress/`.

## Design principles (priority order)

When making changes, follow these in order:

1. **One design:** One authority, type, CLI, journal, and planner per job. If two ways exist, delete the old in the same change (no shims, dual-write, feature flags, or “keep for now”). Update callers, tests, and `specs/` together. Amend [specs/decisions.md](specs/decisions.md) when lock text keeps a fork alive. Skill: `.cursor/skills/hard-cutover`.

2. **Design by construction / compile-time guarantees:** Prefer types, enums, and trait bounds over runtime checks. If an invariant can be enforced by the compiler, it must be. Use Rust’s type system (`enum` variants, `Option`, `Result`, newtypes) to make illegal states unrepresentable. Feature-gated code (`#[cfg(feature = "...")]`) must compile cleanly when enabled. Always favour compile errors over runtime convention. [specs/style.md](specs/style.md), **R2**.

3. **Design simplicity:** Favour the simplest design that satisfies the requirements. Avoid over-abstraction. A flat function is better than a trait hierarchy with one implementor. If a module exists only to re-export, remove the indirection. Simple, consistent, clean code beats clever code.

4. **Tiger Style (ReAct dialect):** Heavily inspired by TigerBeetle Tiger Style (safety, bounds, production asserts, explicit effects) and the Skippr Cloud dialect. Runtime is **async, event-based, and multi-threaded** — not single-threaded. **Kernel** durable mutations MUST be all-or-nothing and effectively exactly-once under retry, using monotonics and optimistic concurrency. **Host/adapter** delivery MAY be at-least-once (network, LLM) — never blur the layers. **Auto-tune** pools/channels/batches/depths/memory — no impl knobs; product config stays explicit. Kernel has **no external side effects**: clock, entropy, storage, LLM, and net are injected so the **same owner code** runs under a deterministic simulator. Normative: [specs/tiger-style.md](specs/tiger-style.md), **R1**. Skill: `.cursor/skills/tiger-style`.

5. **DRY:** Extract shared logic into functions, traits, or shared modules. Duplicated error messages, validation, or serialization patterns are bugs waiting to diverge.

6. **Reuse into that one design:** Prefer extending an existing crate over a new package. Reuse MUST NOT mean keeping a second path, adapter, or “legacy” module beside the chosen design.

7. **TDD + full verification stack:** Write the failing test first. Every kernel/host behaviour change needs the matching layers from [specs/testing.md](specs/testing.md): **unit**, **functional**, **deterministic simulation**, **property**, and **fuzz**. Sleep, wall-clock races, and live network are not correctness. Skill: `.cursor/skills/tdd-react`. **R5**.

## Kernel purity (R4)

`react-core` (agent loop, session, budget, transcript, tool protocol) **MUST NOT** perform external side effects. Inject `Clock`, `Entropy`, `Storage`, `Llm`, and `Net`. Production adapters and the deterministic simulator implement those traits. **MUST NOT** a second test-only commit path.

**MUST NOT** in the kernel: `std::env`, real filesystem, sockets, `Instant::now`, `thread::sleep`, OS RNG as authority.

## Testing (required)

| Layer | Role |
|-------|------|
| Unit | Pure functions; no I/O |
| Functional | Suite/runtime contracts with in-process fakes |
| Deterministic simulation | Seeded clock/llm/net/storage; same kernel; no sleep |
| Property | `proptest` over codecs, budgets, keyspace, idempotency |
| Fuzz | `cargo fuzz` on codecs and simulator schedules; feeds the checker |

CI runs unit + functional + simulation + a bounded property/fuzz slice. Live LLM/S3 jobs are optional and skip (not fail) without credentials.

## Smoke vs larger suite tests

Smoke **this repository** with a simple **in-repo** example suite. Do not pull Skippr Data Engineer into the ReAct inner loop (R6).

| Kind | Suite | Where |
|------|--------|--------|
| **Smoke / ReAct inner loop** | `react-suite-kb` (suite id `kb`) | `src/suites/kb` |
| **Larger / product tests** | `react-suite-data-engineer` | sibling [`sde/crates/skipprd-react-suite-data-engineer`](../sde/crates/skipprd-react-suite-data-engineer) |

Other in-repo suites (`suite_debugger`, `goggles_review`) are also valid ReAct-local targets. Run DE coverage from the `sde` host, not by copying that crate here. The DE suite follows this same ReAct dialect as a **host** (construction, one design, Tiger Style host layer, TDD): sibling `sde/AGENTS.md` and `sde/crates/skipprd-react-suite-data-engineer/docs-internal/react-style-audit.md`.

## Documentation (three layers)

| Layer | Where | Audience |
|-------|--------|----------|
| **Normative** | `specs/` | Engineers — MUST / MUST NOT |
| **Internal** | `docs-internal/` | Maintainers — audits, notes |
| **Public** | `docs/` | Customers — how to use the runtime |

Behavior changes MUST update the matching spec in the same change. Do not put MUST language only in `docs-internal/`.

## Build, test, and lint

```bash
cargo build
cargo test --workspace
cargo fmt --all -- --check
cargo clippy
cargo check --all-features
```

- Toolchain: `rust-toolchain.toml` (currently 1.91.0 + rustfmt/clippy).
- `protoc` is required at compile time by Arrow/Lance-related crates.
- Do not add `-D warnings` to clippy unless asked.
- Memory-constrained linking: test builds can be heavy; prefer `CARGO_BUILD_JOBS` over killing the machine.

Install git hooks with `./scripts/install-git-hooks.sh` (OpenAPI generator drift).

## System dependencies

- Rust via `rust-toolchain.toml`
- `protobuf-compiler` (`protoc`)
- `libssl-dev` / LibreSSL headers when building Lance-related crates

## Agent checklist

1. Read `specs/` ([decisions.md](specs/decisions.md), [tiger-style.md](specs/tiger-style.md), [style.md](specs/style.md), [testing.md](specs/testing.md)) before inventing types or I/O.
2. Encode invariants in types; delete dead/legacy paths in the same change.
3. Keep the kernel pure. New effects go through injected traits, not ambient OS.
4. Tests first: unit + functional; simulation for state machines; property/fuzz for codecs and schedules. Smoke this repo with `src/suites/kb`; larger tests use sibling `sde/crates/skipprd-react-suite-data-engineer`.
5. Durable kernel paths: fixed safety ceilings, auto-tuned ops params, idempotency/dedupe, OCC/monotonics.
6. No impl env knobs (`FOO_BATCH_SIZE`, `WORKER_THREADS`, …). Product config stays explicit on the host.
7. `cargo fmt`, `cargo test -p <crate>`, `cargo check --all-features` when touching features.
