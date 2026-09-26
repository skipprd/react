# Testing (Skippr ReAct)

Normative verification for this repository. Locked by [decisions.md](./decisions.md) **R4–R5**. Code **MUST** be written to support this stack — tests are not an afterthought bolted onto effectful kernel code.

Agents: skill `.cursor/skills/tdd-react`, rule `.cursor/rules/tdd.mdc`. Style: [tiger-style.md](./tiger-style.md) §9.

## 1. Required layers

A kernel or host change is **incomplete** until the relevant layers exist. Not every change needs every layer; the table is the default. Skipping a layer requires a named reason in the PR (generated code, docs-only).

| Layer | Purpose | External side effects | Default location |
|-------|---------|------------------------|------------------|
| **Unit** | Pure functions, constructors, parsers, codecs | **None** | `src/**` `#[cfg(test)]` / crate `tests/` |
| **Functional** | Suite/runtime contracts with fake adapters | **None** (in-process fakes) | crate `tests/` or `tests/functional/` |
| **Deterministic simulation** | Seeded schedules over the **production** kernel | **None** — injected clock/llm/net/storage/entropy | `tests/sim/` (workspace) |
| **Property** | Algebraic / codec / budget invariants over generated inputs | **None** | `proptest` in the crate under test |
| **Fuzz** | Mutate codecs and simulator schedules; feed the **checker** | **None** | `fuzz/` (`cargo fuzz`) |

Live LLM, S3, or network tests **MAY** exist as ignored / feature-gated extras. They **MUST NOT** be the correctness suite.

## 1.1 Smoke vs product-suite tests

- **Smoke this repository** with an in-repo example suite. Default: `react-suite-kb` (`src/suites/kb`, suite id `kb`). Other in-repo suites (`suite_debugger`, `goggles_review`) **MAY** be used. Smoke **MUST NOT** require the Data Engineer suite.
- **Larger / product tests** use the Data Engineer suite in the sibling SDE repo: `sde/crates/skipprd-react-suite-data-engineer` (crate `react-suite-data-engineer`). Those tests **MUST** run from `sde`, not by vendoring DE into this workspace (**R6**).

## 2. TDD

1. Write the **failing test first**. Do not implement feature code without it.
2. Red → green → refactor.
3. Hard-cutover obsolete tests with the code. If a test encoded the discarded path, replace it rather than keeping both.
4. Spec MUST → failing named test → production code the simulator also runs.

## 3. Unit tests

- Cover type construction, parsing, JSON repair, keyspace, schema, budget arithmetic, transcript folds.
- **MUST NOT** touch filesystem, network, env, or sleep.
- **MUST NOT** `unwrap` the success path being asserted — assert `Result` / `Option`.

## 4. Functional tests

- Exercise suite registration, WS request/response shapes, headless run contracts, adapter trait implementations.
- Use in-memory storage, scripted LLM, and fake clocks.
- Cover success **and** reasonable failure permutations (approval interrupt, budget exhausted, OCC conflict, corrupt snapshot).
- **MUST NOT** require Docker, cloud credentials, or a real model.

## 5. Deterministic simulation testing

The kernel is a state machine. The simulator is how we test it.

### Construction rules

- Injected **Clock**, **Entropy**, **Storage**, **Llm**, **Net** (and capabilities). No other I/O.
- One **seed** produces one **trace**. Replay MUST bit-match for a given seed and binary.
- Time advances only when the simulator says so. **MUST NOT** `thread::sleep` / `tokio::time::sleep` as correctness.
- Multi-threading is a **simulated schedule** (explicit interleavings), not a wall-clock race.
- Faults are first-class: crash between `EffectStarted` and `EffectReceipt`, duplicate delivery, OCC conflict, truncated JSON, UTF-8 boundaries, budget exhaustion, lost wakeup.
- The checker asserts invariants on every step: typed transcript, budget monotonicity, no silent lost updates, termination under `RunBudget`, OCC.

### MUST NOT

- A second test-only implementation of the commit / run-loop path.
- Hidden real time, real RNG (`uuid::Uuid::new_v4` in kernel without injected entropy), `std::env`, or ambient statics that diverge across replay.
- Flaky retries (`#[ignore]` to hide nondeterminism). Fix the kernel or the test.

## 6. Property testing

Use `proptest` (or equivalent) on:

- Codecs (WS frames, tool JSON, schema registry, json_repair).
- Keyspace path encoding / round-trip.
- Budget meter: charges never exceed ceilings; exhausted variants are total.
- Transcript / seq: append-only `Seq`, no dropped newest observation when trimming.
- Idempotency: apply(command) twice = apply once.

Properties **MUST** be deterministic (proptest RNG is seeded). Shrinking MUST land on a named fixture when a bug is found.

## 7. Fuzz testing

- `cargo fuzz` targets for wire codecs, json_repair, and **simulator schedules** (byte stream → fault/schedule).
- Fuzz **MAY** mutate inputs and schedules.
- Fuzz **MUST NOT** replace the simulation checker. A crashing fuzz input becomes a **named** regression test (unit, property, or sim).
- Corpus lives in-repo under `fuzz/corpus/` when fixtures are small and non-secret.

## 8. Layout

```
react/
├── src/<crate>/src/**          # unit tests next to code
├── src/<crate>/tests/          # crate-level functional tests
├── tests/functional/           # workspace functional tests (optional)
├── tests/sim/                  # deterministic simulator + checkers
├── fuzz/                       # cargo-fuzz targets
└── specs/testing.md            # this file
```

Shared fakes (scripted LLM, memory storage, fake clock) belong in `react-core` test support or a dedicated `react-test-support` crate — **one** place, used by unit, functional, and sim. Do not copy fakes per crate.

## 9. Commands

```bash
cargo test --workspace                  # unit + functional (no live I/O)
cargo test -p react-core --lib
cargo test --test sim                   # deterministic simulation (when the sim harness exists)
cargo test --features proptest          # or `PROPTEST_CASES=… cargo test -p react-core`
cargo fuzz run codec -- -max_total_time=60
cargo fmt --all -- --check
cargo clippy
cargo check --all-features
```

CI **MUST** run unit + functional + simulation + a bounded property/fuzz slice. Live-provider jobs are optional and skipped (not failed) without credentials.

## 10. Completion gate

A behaviour change in the kernel or a durable host path is not done until:

1. Failing test written first (the layer that would have caught the bug).
2. Production code the simulator runs (no fork).
3. Unit and/or functional coverage for the named case.
4. Property or fuzz when the surface is a codec, budget, schedule, or round-trip.
5. Specs/docs updated if behaviour changed.

**MUST NOT** ship a kernel path that can only be tested by calling a real LLM or sleeping.
