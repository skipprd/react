---
name: tdd-react
description: >-
  Test-driven development for Skippr ReAct: write failing tests first, then
  implement until green. Requires unit, functional, deterministic simulation
  (no external side effects), property, and fuzz coverage. Use when adding or
  changing react-core, runtime, transport, suites, adapters, or verification.
---

# TDD for Skippr ReAct

Normative: [specs/testing.md](../../../specs/testing.md), **R5**. Rule: `.cursor/rules/tdd.mdc`.

## Rules

1. **Red → green → refactor.** No feature code without a failing test written first.
2. **Full stack.** Unit + functional always. Deterministic simulation for kernel/state machines. Property tests for codecs/budgets/keyspace/idempotency. Fuzz on codecs and simulator schedules — fuzz feeds the checker, it does not replace it.
3. **Same owner code.** The simulator runs production kernel code. MUST NOT a second test-only commit path.
4. **No external side effects** in unit, functional, sim, property, or fuzz. Injected clock/llm/net/storage/entropy. MUST NOT `sleep` as correctness. Live LLM/S3 MAY exist as ignored extras; they are not the suite.
5. **Hard cutover.** Delete obsolete tests with obsolete code; no dual suites.
6. **Durable paths.** Cover retry/idempotency and OCC-conflict (skill **`tiger-style`**).
7. Shared fakes in one place (core test support or `react-test-support`).

## Layout

| Kind | Location |
|------|----------|
| Unit | `src/<crate>/src/**` `#[cfg(test)]` |
| Functional | crate `tests/` or `tests/functional/` |
| Deterministic simulation | `tests/sim/` |
| Property | `proptest` in the crate under test |
| Fuzz | `fuzz/` (`cargo fuzz`) |

## Workflow

1. Read `specs/` (tiger-style, testing, style).
2. Write the failing test at the layer that would have caught the bug.
3. Implement the minimum to pass — production code the simulator runs.
4. Add property/fuzz when the surface is a codec, budget, schedule, or round-trip.
5. `cargo test -p <crate>` (and sim/property as applicable) before moving on.

## Commands

```bash
cargo test -p <crate>
cargo test --workspace
cargo check --all-features
# when the harness exists:
# cargo test --test sim
# cargo fuzz run codec -- -max_total_time=60
```

## Suites under test

- Smoke **this repo** with an in-repo example suite (`react-suite-kb` in `src/suites/kb`). Do not register Data Engineer for ReAct smoke.
- Larger product tests use `sde/crates/skipprd-react-suite-data-engineer` (`react-suite-data-engineer`) from the `sde` host. See `AGENTS.md` and [specs/testing.md](../../../specs/testing.md) §1.1.

## Anti-patterns

- Implementing first, tests later
- Sleep / wall-clock races / live model calls as correctness
- `#[ignore]` to hide nondeterminism
- Copy-pasted fake clocks per crate
- Fuzz without a checker
