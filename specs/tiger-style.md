# Tiger Style (ReAct dialect)

Normative engineering style for Skippr ReAct. Locked by [decisions.md](./decisions.md) **R1**. Heavily inspired by [TigerBeetle Tiger Style](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md) and the Skippr Cloud dialect (`cloud/specs/tiger-style.md`); not a copy of TigerBeetle’s single-threaded ledger runtime.

Agents: root [AGENTS.md](../AGENTS.md), rule `.cursor/rules/tiger-style.mdc`, skill `.cursor/skills/tiger-style`. Style details: [style.md](./style.md). Verification: [testing.md](./testing.md).

## 1. Intent

Adopt Tiger Style’s emphasis on **safety**, **fixed bounds**, **production asserts**, **simplicity**, **explicit control flow**, and **design by construction**.

**Intentional divergences from TigerBeetle:**

| TigerBeetle (typical) | Skippr ReAct |
|-----------------------|--------------|
| Single-threaded state machine | **Async, event-based, multi-threaded** (Tokio) |
| One OS thread owns the world | Logical ownership via **types + monotonics + OCC** |
| Minimal external I/O in-core | Side effects **only** through injected world traits — explicit commit boundaries |
| VOPR / simulator as engine TDD | Deterministic simulation of the **same** kernel the host runs |

## 2. Two layers (MUST distinguish)

| Layer | Meaning | Guarantee |
|-------|---------|-----------|
| **Kernel** | `react-core`: agent loop, transcript, session, budget, tool protocol, suite contract | Pure given an injected world. Durable mutations **effectively exactly-once** under retry. **The simulator runs this code.** |
| **Host / adapters** | `react` runtime, `react-transport`, storage/LLM/vector adapters, product hosts | Real I/O. Product delivery MAY be at-least-once (network, LLM). MUST NOT weaken kernel purity or EOS. |

**MUST NOT** claim the product is exactly-once because the kernel is. **MUST NOT** put `std::env`, wall-clock, filesystem, or network inside the kernel so “the host is simpler.”

## 3. Design by construction (R2)

If an invariant can be enforced by the compiler, it **MUST** be.

- Illegal states unrepresentable: `enum` modes, newtype ids, `Option`/`Result`, non-zero budgets.
- Typed errors at crate boundaries. Stringly `anyhow` sprawl is forbidden in `react-core`.
- **Host retry (R8):** LLM failures are `LlmError`; storage failures are `StorageError`; backoff is `retry_classified`. Provider HTTP is one-shot. **MUST NOT** encode errors as `Ok` prefixes, classify storage by substring, grep model-name prefixes outside `openai_api_for_model`, or keep a second sleep/backoff loop in an adapter.
- **Run budget (R9):** `BudgetMeter` charges LLM/tool/agent steps; `Cancel` aborts awaits; `WorkflowError` is the phase-loop error. **MUST NOT** `headless_budget_exhausted` strings or saturating replan-backtrack.
- **Transcript (R10):** `Transcript::render(PromptBudget)` emits real `ChatRole`s. Tool success is `Result::Ok` / `ToolOutcome::Ok`. **MUST NOT** prefix-sniff `Vec<String>` or treat missing JSON `"ok"` as failure.
- Feature-gated code MUST compile when enabled (`cargo check --all-features`).
- Always favour compile errors over runtime convention.
- A runtime “don’t call this wrong” comment is a missing type.

## 4. Runtime model

- Kernel and host MUST be **async** and **event-driven**.
- **Multi-threading** is allowed and expected.
- **MUST NOT** require process-wide single-threading as a correctness crutch.
- Prefer logical single-owner per thread/run enforced by **OCC / monotonic `Seq`**, not by pinning all work to one OS thread.
- Logical time is an injected **Clock**. Wall time is not an authority.

## 5. Exactly-once / all-or-nothing (kernel durable paths)

Applies to thread store, audit log, budget meter, and retryable command handlers — not automatically to LLM token streams.

- Every retryable handler MUST use an **idempotency key** (or natural key) and durable **dedupe / stored result**.
- **Commit boundary:** state change and published side effect succeed together or neither.
- Retries MUST be safe. Partial durable state after failure is a bug.
- “Effectively exactly-once” means: under at-least-once delivery of the *trigger*, the *effect* applies once.

External effects that cannot share a transaction with the journal (LLM call, tool subprocess, object write) MUST use a receipt-backed state machine:

1. persist `EffectStarted` with a deterministic effect id through OCC;
2. execute idempotently or by a natural provider idempotency key;
3. verify and durably persist the external postcondition as `EffectReceipt`;
4. only then advance the phase through OCC.

After a crash between effect and receipt, retry MUST probe the postcondition before repeating the call. An in-memory “already ran” flag is not an exactly-once boundary.

## 6. Monotonics and optimistic concurrency

- Prefer monotonic fields (`Seq`, `version`, `epoch`, run generation).
- Concurrent kernel writes MUST use **conditional put/update**. On conflict: abort and retry with a fresh read.
- **MUST NOT** last-write-wins on concurrent thread/store mutations unless a named decision locks that case.
- In-memory maps (`DashMap`, `HashMap`) are not durability or OCC.

## 7. Tiger Style practices we adopt

- **Assert invariants** on production paths (`assert!` or typed fail-closed errors at trust boundaries). Silent fallback on protocol, tenancy, or budget invariants is forbidden.
- **Fixed safety limits (ceilings):** max LLM calls, max tool calls, max prompt tokens, max transcript bytes, max in-flight tools, max auto-resumes — document and enforce **in code**. Working setpoints (pools, batches, channel depths) MUST be auto-tuned (§8), not operator-configured.
- **Illegal states unrepresentable** — aligns with R2 and [style.md](./style.md).
- **Explicit effects:** decide under checks → ordered effects → execute with timeouts/limits → cheap postconditions where practical.
- **Pure planning:** desired state + typed durable operation + immutable observations → typed decision. Discovery and planning perform no effects; one executor owns effects and receipts.
- **Shallow modules;** no trait hierarchy with a single implementor.
- **Injected world:** `Clock`, `Entropy`, `Storage`, `Llm`, `Net` (and suite capabilities) are traits. Kernel code MUST NOT call `std::time::Instant::now`, `thread::sleep`, `std::env`, real RNG, or real sockets.

## 8. Auto-tuning (platform internals)

**Platform-internal operational parameters MUST auto-tune.** Do not expose a thousand env/config knobs on the runtime.

| Allowed | Forbidden as operator knobs |
|---------|-----------------------------|
| Host **product** config (model id, suite registration, storage URI, listen bind) | Thread/worker counts, Tokio pool sizes |
| Deployment identity wiring (port, cert path) | Channel / mpsc depths, internal queue depths, buffer sizes |
| Hard **safety ceilings** as code constants (budgets, max payload, max page) | Batch sizes, flush intervals, memory soft targets |

**Auto-tune means:** derive from available CPUs, cgroup/memory, and observed load; adapt within hard safety ceilings. Ceilings stay **fixed in code**; the working setpoint moves.

Buffers and channels MUST remain **bounded**. Auto-tune chooses the bound; it does not remove bounds.

## 9. Deterministic simulation (R4, R5)

The kernel **MUST** be written so a seeded simulator can drive it with **no external side effects**.

- Spec MUST → failing named test → production code the **simulator also runs**.
- Injected clock / log / net / LLM / storage. Seeded replay.
- **MUST NOT** `sleep` as correctness. **MUST NOT** depend on wall-clock races.
- Multi-threading is a **simulated schedule**, not a flaky wall-clock race.
- Fuzz **MAY** mutate codecs and simulator schedules; it **MUST NOT** replace the checker.
- **MUST NOT** a second test-only implementation of the commit path.

Detail: [testing.md](./testing.md).

## 10. Anti-patterns

- Best-effort dual-write without dedupe (kernel).
- Catch-and-ignore on protocol / tenancy / budget invariants.
- Unbounded buffers or channels.
- `std::env` / filesystem / network / `Instant::now` inside `react-core`.
- Process-wide single-thread mandates “for Tiger Style.”
- Env sprawl for impl tunables (`FOO_BATCH_SIZE`, `WORKER_THREADS`, `CHANNEL_DEPTH`, …).
- Sleep-based tests; a second test-only commit path.
- Sniffing roles out of `Vec<String>` prefixes when a typed transcript exists.
- Panics on valid-but-unexpected JSON, UTF-8 boundaries, or missing map keys.

## 11. Applies / does not apply

| Applies | Does not apply as dogma |
|---------|-------------------------|
| `react-core`, durable session/store, agent loop, suite contract | Product host CLI copy |
| Shared adapters’ durable paths | Opportunistic bash scripts (still prefer idempotent ops) |
| Transport codec + WS protocol state machines | TigerBeetle static-only allocation / single-thread runtime |
| Agent rules when editing kernel or host platform code | Live LLM calls as a substitute for simulation |

## Related

- [decisions.md](./decisions.md) R1–R5
- [style.md](./style.md) — Rust construction rules
- [testing.md](./testing.md) — unit, functional, simulation, property, fuzz
- [layout.md](./layout.md)
