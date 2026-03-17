## ReAct (`react` crate)

A **WebSocket-based ReAct agent runtime**. Clients send JSON frames, the server routes each request to a **suite**, and the suite implements a **`PhaseExecutor`** driven by a generic **workflow runner** in core. Each workflow turn runs a **ReAct loop** (LLM → tool calls → observations → complete/interrupt), using injected providers (query/catalog/vector/dbt/storage). Sub-workflows (e.g. repair) nest inside outer turns using the same runner pattern.

### Getting started

- **MSSQL → Snowflake** (bronze tier, local storage): see [`getting-started.md`](getting-started.md)
- Windows + BigQuery (local storage): see `GETTING_STARTED_WINDOWS_BIGQUERY.md`

### Prerequisites

#### Python virtual environment & dbt

The runtime shells out to `dbt` for project scaffolding and validation. Install it in an isolated venv so its dependencies don't conflict with other system packages:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install --upgrade pip
pip install dbt-core dbt-athena-community
```

Replace `dbt-athena-community` with the adapter for your warehouse (e.g. `dbt-bigquery`, `dbt-postgres`, `dbt-snowflake`).

To install dbt packages declared in a project's `packages.yml`:

```bash
cd dbt-examples/mobile_app_analytics
dbt deps
```

Make sure the venv is activated (`source .venv/bin/activate`) whenever you run the server or tests that invoke dbt.

### Run the server

From the repository root:

```bash
cargo run -p react -- serve --config src/runtime/config.example.yml --port 8787 --terminal
```

The server speaks WebSocket on `ws://localhost:8787/` using schemas in `src/runtime/openapi/ws-core.yaml` plus suite overlays.

Notes:
- By default, storage is **local** (`storage.mode: local`) and persists under `storage.path` (default `./.react`).
- To use S3 storage, pass `--storage-mode s3 --bucket <BUCKET>` (or set `storage.mode: s3` + `storage.bucket` in YAML / env `SKIPPR_S3_BUCKET`).

### Configuration notes (LLM output size)

The **effective output token limit** is controlled by the environment variable **`LLM_MAX_TOKENS`**.

- When you run `react serve` with a YAML config (e.g. `src/runtime/config.example.yml`), the loader in `src/runtime/src/config.rs` will **set `LLM_MAX_TOKENS` from `llm.max_tokens` if it is not already set**.
- If you see errors like `parser_error invalid JSON twice` during large batch scaffolds, your model output is likely being **truncated**. Increase `llm.max_tokens` (or set `LLM_MAX_TOKENS` explicitly) so tool-call JSON can fit (for `gpt-5.x`, **8192** is a reasonable starting point).

---

## Architecture

### High-level overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Client (UI / CLI)                              │
│                         WebSocket JSON frames in/out                        │
└────────────────────────────────────┬────────────────────────────────────────┘
                                     │
                    ┌────────────────▼────────────────┐
                    │     WebSocket Server (runtime)   │
                    │  frame routing · thread lifecycle │
                    │  suite dispatch · response stream │
                    └────────────────┬────────────────┘
                                     │
              ┌──────────────────────▼──────────────────────┐
              │                Suite (react-suites)                │
              │                                                    │
              │  Phase state machine · prompt construction ·       │
              │  tool selection · LLM planning calls ·             │
              │  phase transition logic                            │
              │                                                    │
              │  ┌──────────────────────────────────────────┐     │
              │  │          Agent Loop (react-core)          │     │
              │  │                                           │     │
              │  │   LLM call → parse → tool exec → observe │     │
              │  │   policy gates completions & interrupts   │     │
              │  │   schema-validated JSON actions            │     │
              │  └──────────────────────────────────────────┘     │
              └──────────────────────┬──────────────────────┘
                                     │
         ┌───────────────────────────┼───────────────────────────┐
         │                           │                           │
    ┌────▼────┐               ┌──────▼──────┐            ┌──────▼──────┐
    │  Tools  │               │  Providers  │            │  Storage    │
    │         │               │  (injected) │            │  (threads,  │
    │ dynamic │               │ query·dbt·  │            │  artifacts, │
    │ per-suite│              │ catalog·    │            │  state)     │
    │ registry│               │ vector·     │            │             │
    └─────────┘               │ warehouse   │            └─────────────┘
                              └─────────────┘
```

### Crate structure

```
react (workspace root)
│
├── src/core            (react-core)      Generic loop, traits, session, workflow — no infra or domain deps
│   └── src/workflow/                     Workflow runner: PhaseOutcome, PhaseExecutor trait, run()
├── src/transport       (react-transport) WS protocol/server + headless transport
├── src/runtime         (react)           CLI/bootstrap + concrete provider wiring
├── src/suites          (react-suites)    Suite implementations
│   └── lib.rs + one directory per suite (data_engineer/, kb/)
│       └── data_engineer/
│           ├── providers/                Suite-specific provider traits
│           ├── de_config.rs              Suite-specific config types
│           ├── ctx_ext.rs                Capability accessors (newtypes + helpers)
│           └── modules/                  Suite-specific module crates (src/data_engineer/modules/)
│               ├── provider-athena       Athena warehouse
│               ├── provider-bigquery     BigQuery warehouse
│               ├── provider-postgres     Postgres warehouse
│               └── provider-dbt          dbt CLI wrapper
│
└── src/modules                           Generic (suite-agnostic) modules
    ├── storage                           S3 adapter
    └── provider-vector-lance             LanceDB vector store
```

**Dependency rule**: `react-core` has zero workspace dependencies and **zero domain-specific types**. Everything depends inward on `react-core`. `react-core` provides the generic workflow runner (`PhaseOutcome`, `PhaseExecutor`, `WorkflowConfig`, `workflow::runner::run()`), the agent ReAct loop, tools, session, and provider traits. `react-runtime` wires concrete providers and hands execution to `react-transport`. Suites are fully self-contained — they depend only on `react-core`, never on each other. Domain concepts (phase transitions, guard kinds, review decisions, provider traits like `WarehouseProvider`/`CatalogProvider`/`DbtProvider`/`QueryProvider`, thread caches) live exclusively in the suite that owns them.

**Two-tier module architecture**: modules that are specific to a single suite (e.g. warehouse adapters, dbt CLI wrapper) live under that suite's directory and depend on suite-local traits. Modules that are truly suite-agnostic (e.g. S3 storage, vector store) live at the top-level `src/modules/` and depend only on `react-core`.

```
runtime ──► transport ──► react-core ◄── react-suites
  │             │              ▲              │
  └──► modules ─┴──────────────┘              └── suite modules (depend on suite traits)
       (generic)
```

---

### Layer-by-layer

#### 1. Transport — WebSocket server

**Crate**: `react-transport` · **File**: `src/transport/src/ws/server.rs`

The WS server is a thin routing layer. It never calls the agent loop directly.

- Parses inbound frames (`new`, `open`, `user`, `approve`, `delete`, etc.)
- Creates/loads threads and persists user steps
- Resolves `suite_id` + `agent_type` via `ControlStateStore`, with ThreadLog scan as recovery fallback
- Converts the suite's `FlowFrame` output into wire-format `ServerMessage`s and streams them back
- Constructs the `SuiteCtx` (injected capabilities) for each request via the runtime's provider wiring

**`FlowFrame` → wire mapping:**

| FlowFrame | ServerMessage |
|-----------|---------------|
| `Complete { kind: String, payload, display }` | `Final` |
| `Review { text, meta }` | `Review` |
| `Checkpoint { kind, payload }` | `Review` (checkpoint surfaced as review) |
| `Interrupt { kind: "await_approval", prompt }` | `AwaitApproval` |
| `Interrupt { kind: "await_user", prompt }` | `AwaitUser` |

#### 2. Suites — product surfaces

**Crate**: `react-suites` · **Files**: `src/suites/react-suites/src/<suite_name>/`

A suite is a self-contained product workflow. Each suite directory owns everything it needs: prompts, tools, policies, phase logic, preflight, and dbt helpers. Suites depend only on `react-core` traits, never on each other.

A suite owns:

| Concern | Description |
|---------|-------------|
| **Phase order** | An enum of phases forming a state machine |
| **Domain types** | Control flow enums (`PhaseTransition`, `GuardBlockKind`, `ReviewDecision`, etc.) in `domain_types` + `progress_controller` modules |
| **Prompts** | System prompt and tool card per phase |
| **Tool registry** | Which tools are available in each phase |
| **Policy** | An `AgentPolicy` that gates completions and interrupts |
| **Planning LLM calls** | Pre-loop LLM work (design memos, enrichment, critique) via `AgentCtx::llm_chat` |
| **Phase transition logic** | Rules for advancing, looping back, or blocking |
| **Pre-turn guards** | `evaluate_pre_turn_directive` and `PreTurnStateSnapshot` — prevents runaway phases |
| **Thread cache** | Per-thread in-memory caching (e.g. `ThreadCache`/`ThreadCacheStore` in `data_engineer/thread_cache.rs`) |
| **Keyspace extensions** | Domain-specific key helpers (`catalog_key`, `semantic_key`, `dbt_prefix`, `lancedb_uri`, etc.) built on core's generic `scoped_key`/`scoped_prefix` |
| **Agent types** | Different modes of the same suite (e.g. interactive ask vs autonomous agent) |

Registered suites:
- **`data_engineer`** — multi-phase analytics + dbt workflow (plan → author → validate → review → publish)
- **`kb`** — knowledge-base Q&A

Suites return `Vec<FlowFrame>` to the transport:

```rust
enum FlowFrame {
    Complete   { kind: String, payload, display },   // terminal result
    Review     { text, meta },                       // review checkpoint
    Checkpoint { kind, payload },                    // internal checkpoint
    Interrupt  { kind, prompt },                     // pause for user/approval
}
```

#### 3. Contexts — `SuiteCtx` vs `AgentCtx`

Two context structs carry capabilities through the system. Understanding the boundary between them is key:

**`SuiteCtx`** is the suite's "world" — everything the runtime injects at request time:

```
SuiteCtx
├── storage          StorageAdapter (S3 / local FS)
├── scope            RequestScope (tenant / workspace / project)
├── keyspace         Keyspace (persistence key layout)
├── secrets          SecretsProvider
├── llm              Raw LLM handle
├── resolved_config  Parsed YAML config (suite_config: Value for domain-specific)
├── vector?          VectorStore
├── state?           StateStore
├── capabilities     HashMap<TypeId, Arc<dyn Any>>  ← suite-specific providers
└── trace_tx?        Trace channel
```

**`AgentCtx`** is what the suite builds *per phase* for the agent loop. It narrows `SuiteCtx` to what a single ReAct invocation needs:

```
AgentCtx
├── llm              LLM handle (from SuiteCtx)
├── policy           AgentPolicy (suite-chosen, phase-specific)
├── storage          StorageAdapter
├── scope / keyspace (from SuiteCtx)
├── vector?          VectorStore (generic, in core)
├── capabilities     HashMap<TypeId, Arc<dyn Any>>  ← suite-specific providers
├── thread_store?    ThreadStore (for transcript persistence)
├── max_steps        Step limit for this phase
├── per_step_timeout_secs
├── resolved_config  (from SuiteCtx)
└── exec_ctx?        ExecutionContext (for hierarchical UI rendering)
```

**Capability registry pattern**: both `SuiteCtx` and `AgentCtx` carry a generic `capabilities: HashMap<TypeId, Arc<dyn Any + Send + Sync>>` map. Suite-specific providers (e.g. `WarehouseProvider`, `DbtProvider`, `QueryProvider`, `CatalogProvider`) are stored as typed capabilities using wrapper newtypes (e.g. `WarehouseCap(Arc<dyn WarehouseProvider>)`). Helper functions in `data_engineer/ctx_ext.rs` provide ergonomic typed access:

```rust
let wh = sctx_warehouse(sctx).unwrap();
let dbt = actx_dbt(ctx).unwrap();
```

The `SuiteCtx → AgentCtx` construction is where the suite sets the policy, step limits, and tool registry for each phase. Capabilities are copied from `SuiteCtx` to `AgentCtx` via `copy_capabilities_to_actx()`. The agent loop itself never sees `SuiteCtx`.

#### 4. Agent types

A single suite can support multiple **agent types** (modes of operation). The client specifies the agent type when sending a message; the suite uses it to select different tools, policies, prompts, and phase subsets.

The `data_engineer` suite supports:

| Agent type | Behaviour |
|------------|-----------|
| `agent` | Autonomous multi-phase workflow (plan → author → validate → review → publish). Non-interactive — the agent never pauses to ask the user. |
| `ask` | Interactive analytics Q&A. Single-phase: user asks, agent queries data, returns an answer. |
| `review` | Read-only project review. Inspects dbt artifacts and returns a structured assessment. |
| `model` | Focused modeling workflow (gold-tier authoring). |
| `cleanse` | Focused cleansing workflow (silver-tier authoring). |

Agent types determine which `AgentPolicy` is used (e.g. `NonInteractivePolicyAdapter` for `agent` mode, `InterruptOnlyPolicy` + `SqlValidatedPolicy` for `ask` mode), which tools appear in the tool card, and which phases execute.

#### 5. Workflow runner — generic phase orchestration

**Crate**: `react-core` · **Files**: `src/core/src/workflow/`

The workflow runner is a generic orchestration loop that drives any phase-based workflow. It manages step budgets, idle detection, and outcome routing — without knowledge of any specific suite or domain.

Suites implement the `PhaseExecutor` trait:

```rust
#[async_trait]
pub trait PhaseExecutor: Send + Sync {
    async fn execute_turn(&self) -> PhaseOutcome;
    async fn on_budget_exhausted(&self) -> Vec<FlowFrame>;
    fn step_count(&self) -> usize;
}
```

Each turn returns a `PhaseOutcome`:

| Variant | Meaning |
|---------|---------|
| `StayedWithProgress` | Forward progress — resets budget counter |
| `StayedWaiting` | No progress — increments idle counter |
| `TransitionCommitted` | Phase transition committed — clears idle counter |
| `Return(frames)` | Terminal — returns `FlowFrame`s to caller |
| `Failed(reason)` | Fatal — stops execution |

The runner is configured via `WorkflowConfig`:

```rust
pub struct WorkflowConfig {
    pub max_phase_steps: usize,
    pub max_consecutive_waiting_idle: usize,
    pub max_consecutive_waiting_active: usize,
}
```

`workflow::runner::run(&executor, &config)` is the single entry point. The `data_engineer`, `kb`, and repair subroutine all use this same runner.

#### 6. Agent loop — ReAct runtime

**Crate**: `react-core` · **Files**: `src/core/src/agent/`

The agent loop is a generic ReAct executor. It has no knowledge of suites, phases, or domain logic — it only knows about tools, an LLM, and a policy. It runs *inside* a single workflow turn — the workflow runner calls the suite's `execute_turn`, which builds an `AgentCtx` and invokes the agent loop.

Each invocation receives an `AgentCtx` and runs:

```
┌─────────────────────────────────────────────────────────────────┐
│                        Agent ReAct Loop                         │
│                                                                 │
│  ┌──────────┐    ┌─────────┐    ┌───────────┐    ┌──────────┐ │
│  │ Build    │───►│ LLM     │───►│ Parse     │───►│ Dispatch │ │
│  │ prompt   │    │ call    │    │ response  │    │          │ │
│  │ (system  │    │         │    │           │    │  Tool?   │ │
│  │ + tools  │    │ via     │    │ AgentStep │    │  ──► exec│ │
│  │ + history│    │ AgentCtx│    │ V1 schema │    │     + obs│ │
│  │ + obs)   │    │::llm_   │    │ validated │    │          │ │
│  │          │    │  chat() │    │           │    │Complete? │ │
│  └──────────┘    └─────────┘    └───────────┘    │  ──► pol.│ │
│       ▲                                          │     gate │ │
│       │                                          └────┬─────┘ │
│       │              ◄── observation ─────────────────┘       │
│                                                                 │
│  Terminates when:                                               │
│    • Policy accepts a Complete  → RunOutcome::Complete          │
│    • Policy triggers Interrupt  → RunOutcome::Interrupt         │
│    • Step limit reached         → policy.fallback()             │
└─────────────────────────────────────────────────────────────────┘
```

The LLM returns a **schema-validated JSON action** on every turn:

```json
{ "type": "tool",     "name": "run_sql", "args": { "sql": "..." } }
{ "type": "complete", "complete": { "kind": "...", "payload": { ... } } }
```

Parsing is strict (validated against `AgentStepV1` JSON schema). Invalid JSON triggers up to 2 retries with a reduced prompt before the loop gives up.

#### 7. LLM gateway and model dispatch

**Files**: `src/core/src/agent/llm_gateway.rs`, `src/core/src/llm/`

All LLM calls — whether from the agent loop or from suite planning code — go through gateway methods on `AgentCtx` and `SuiteCtx`:

| Method | Available on | Purpose |
|--------|-------------|---------|
| `llm_chat(messages, options)` | `AgentCtx` | Raw chat completion. Emits `LlmStart`/`LlmEnd`/`LlmCall` thread events automatically. Supports per-call `timeout_secs`. |
| `llm_chat_json<T>(messages, options)` | `AgentCtx` | Calls `llm_chat`, deserialises the response as JSON, auto-repairs malformed JSON (escape control chars), and retries once with a "return only JSON" nudge on failure. |
| `llm_embed(text)` | `AgentCtx`, `SuiteCtx` | Embedding gateway. Suite code uses `SuiteCtx::llm_embed()` outside the agent loop; within the loop, tools use `AgentCtx::llm_embed()`. Both route through the same underlying LLM handle. |

**Model dispatch**: LLM configuration supports per-role model selection via `ModelDispatch`:

| Role | Config key | Purpose |
|------|-----------|---------|
| `reason_model` | `llm.reason_model` | Deep reasoning, planning, critique, review — creative and analytical tasks requiring high-quality output |
| `task_model` | `llm.task_model` | Mechanical tool-calling, gather/investigate stages, compile passes — cheaper, faster tasks requiring reliable JSON |
| `embed_model` | `llm.embed_model` | Embedding for vector store indexing and retrieval |

`ModelDispatch` provides `reason_call_options(prompt_id, iteration)` and `task_call_options(prompt_id)` to construct `LlmCallOptions` with the appropriate model and parameters for each role. This separates expensive reasoning from cheap task execution without per-call configuration.

This eliminates scattered observability code, ensures every LLM interaction appears in the thread timeline, and gives suites a single embed entry point regardless of context.

#### 8. Tools

**Files**: `src/core/src/tools/mod.rs` (trait + registry), suite-specific tools under each suite directory

Tools are async functions registered by name. The suite decides which tools are available per phase.

```rust
#[async_trait]
trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String>;
}
```

`ToolRegistry` is a `HashMap<&str, Box<dyn Tool>>`. The agent loop calls `tools.call(name, args, ctx)`, applies a per-tool timeout (from the policy), persists `ToolStart`/`ToolEnd` thread steps, and appends the result as an observation to the transcript.

#### 9. Policies

**Trait**: `AgentPolicy` in `src/core/src/agent/mod.rs`

Policies let suites control the agent loop's behaviour without modifying it:

| Method | When called | Purpose |
|--------|-------------|---------|
| `handle_complete(...)` | Model emits a Complete action | Accept → `Some(RunOutcome::Complete)`. Reject → `None` (appends an observation, loop continues). Enables validation gates — e.g. require successful `dbt_validate` before accepting. |
| `interrupt_for_action(...)` | After each tool call | Return `Some((InterruptKind, prompt))` to pause execution and hand control back to the user/approver. |
| `timeout_for_tool(name)` | Before tool execution | Per-tool timeout override. |
| `prelude_lines(...)` | Before prompt construction | Extra transcript lines injected before the user's question. |
| `fallback(...)` | Step limit reached | Default: interrupt with "step limit" message. |
| `clean_tool_name(name)` | Prompt rendering | Formats raw tool names for display. Core provides a default (`title_case_words`); suites override for domain-specific formatting. |

Key policy patterns:
- **`InterruptOnlyPolicy`** — converts `ask_user`/`ask_approval` tool calls into interrupts; used in interactive modes.
- **`NonInteractivePolicyAdapter`** — wraps any policy and suppresses all interrupts; the agent never pauses in non-interactive mode (but the user can still inject context into a running thread).
- **`SqlValidatedPolicy`** — gates completion on successful SQL execution / dbt validation.

#### 10. Providers — capabilities injection

Providers are trait objects injected via `SuiteCtx`. The core and suites never import concrete implementations. Provider traits are split into two tiers:

**Generic traits** (in `react-core`, `src/core/src/providers/`):

| Trait | Capability |
|-------|------------|
| `VectorStore` | Embedding upsert/query |
| `StorageAdapter` | Key-value persistence (local FS or S3) |
| `Keyspace` | Generic scoped key layout. Requires only `scoped_key(scope, segments)` and `scoped_prefix(scope, segments)`; default impls cover thread/log keys. Domain-specific key methods (catalog, semantic, dbt, lancedb, etc.) are defined in suites, not core. A free function `encode_key_component()` encodes arbitrary identifiers for key segments. |
| `StateStore` | Execution state persistence |
| `SecretsProvider` | Credential resolution |

**Suite-specific traits** (in `data_engineer/providers/`):

| Trait | Capability |
|-------|------------|
| `QueryProvider` | SQL execution, schema introspection, sampling |
| `WarehouseProvider` | Warehouse metadata + naming (supertype of QueryProvider + DatasetCatalogProvider + WarehouseNaming) |
| `DatasetCatalogProvider` | Dataset discovery and bootstrap |
| `CatalogProvider` | Semantic catalog (descriptions, lineage) |
| `DbtProvider` | dbt project scaffolding, validation, run |

Suite-specific providers are stored in the **capabilities registry** (`HashMap<TypeId, Arc<dyn Any>>`) on `SuiteCtx`/`AgentCtx` using wrapper newtypes. This keeps `react-core` free of domain-specific provider types while providing type-safe access in suite code.

**Concrete impls**: `src/suites/react-suites/src/data_engineer/modules/` (suite-specific) + `src/modules/` (generic) + `src/runtime/src/providers/`

The runtime crate (`src/runtime/src/main.rs`) constructs the concrete provider set and injects it via `set_capability()`. Swapping Athena for BigQuery is a one-line config change — no suite or core code changes.

#### 11. Session, state, and persistence

**Files**: `src/core/src/session/`

The session layer enforces a strict hierarchy:

```text
1. ControlState   — first-class source of truth for execution decisions
2. ThreadLog      — secondary audit log
3. ViewCache / WS — disposable projection for display
```

Every interaction is recorded in `ThreadLog` as an append-only sequence of `ThreadStep`s:

```
ThreadStep::User           — user message
ThreadStep::LlmStart       — LLM call began (prompt hash, call id)
ThreadStep::LlmEnd         — LLM call returned (call id, token count)
ThreadStep::LlmCall        — full LLM interaction (messages, response, metadata)
ThreadStep::ToolStart      — tool invoked (name, args)
ThreadStep::ToolEnd        — tool returned (name, result, duration)
ThreadStep::Phase          — phase transition (reason_code: Option<String>)
ThreadStep::GuardBlock     — guard prevented execution (kind: String)
ThreadStep::ArtifactFocus  — artifact selected for work (entity_id)
ThreadStep::ArtifactSaved  — artifact persisted (entity_id)
ThreadStep::Complete       — terminal result (kind: String)
```

`ThreadStep` uses **strings at the serialization boundary**: `Phase.reason_code` is `Option<String>`, `GuardBlock.kind` is `String`, `Complete.kind` is `String`. Suites maintain compile-time enum safety internally; conversion to/from strings happens at the recording point. This keeps core free of domain-specific enum variants.

`ThreadStore` persists steps as JSON via `StorageAdapter` + `Keyspace`. This gives full observability and replay capability.

Read/write are separated by type and API surface:

| Type | Role |
|------|------|
| `ThreadLogReader` | Read-only log access |
| `ThreadLogWriter` | Log writes (`append_step`, `set_title_if_absent`, `lock_title`, `delete`) |
| `ControlStateStore` | Read/write execution state |

`ThreadLogViewCache` lives in `react-view` and is materialized on demand from `ThreadLog` for WS/headless responses. It is never a source of truth and never drives control flow.

---

### Phase-driven workflow

Suites orchestrate multi-phase workflows. Each phase is a node in a state machine; the suite's own modules define valid transitions and domain-specific types.

Core provides two levels of orchestration:

1. **Workflow runner** (`core::workflow`) — generic `PhaseExecutor` trait + `workflow::runner::run()` loop. Handles step budgets (with progress-based reset), idle detection, and outcome routing. All suites use this single runner.
2. **Suite contract** — `WorkflowSuiteContract` trait with associated types `Phase`, `GuardKind`, `State`, `Event`. `PhaseDirective<P, R, G>` is generic over phase, reason, and guard kind. Domain-specific types like `PhaseTransition`, `GuardBlockKind`, `ReviewDecision`, and `ReviewTier` live entirely in the suite (e.g. `data_engineer/progress_controller.rs`, `domain_types.rs`), not in core. All state mutations flow through a typed `DataEngineerEvent` enum via `apply_event()`, giving compile-time enforcement of valid transitions.

#### `data_engineer` phases (example)

```
┌──────────────────────────────────────────────────────────────────────┐
│                                                                      │
│  Preflight ──► CleansePlan ──► CleanseAuthor ──► CleanseValidate    │
│                    ▲               │                   │             │
│                    └───────────────┘                   │             │
│                  (loopback on                          ▼             │
│                   plan revision)              CleanseReview          │
│                                                       │             │
│                                                       ▼             │
│                ModelPlan ──► ModelAuthor ──► ModelValidate           │
│                    ▲             │                   │               │
│                    └─────────────┘                   ▼               │
│                                              ModelReview            │
│                                                       │             │
│                                                       ▼             │
│                                        PublishAwaitApproval         │
│                                                       │             │
│                                                       ▼             │
│                                    Publish ──► PostPublishReview    │
│                                                       │             │
│                                                       ▼             │
│                                                     Done            │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

When validation fails and the suite enters repair mode, the author phase dispatches to a **repair subroutine** (see below) instead of the normal authoring flow. The subroutine runs its own `PhaseExecutor` loop inside the outer workflow.

Not all agent types traverse the full graph. `ask` mode runs a single phase (no state machine). `review` mode runs only a review phase. The `agent` type drives the full plan → author → validate → review → publish pipeline.

#### How a phase executes

```
Suite.handle_user(thread_id, text, agent_type, ctx)
  │
  ▼
run_agent
  │
  ├── Initial setup (catalog bootstrap, model config, thread store)
  ├── Construct DataEngineerExecutor (implements PhaseExecutor)
  ├── Build WorkflowConfig { max_phase_steps, idle limits }
  │
  └── workflow::runner::run(&executor, &config)
        │
        ├── executor.execute_turn()
        │     ├── Load ExecutionState → current phase
        │     ├── evaluate_pre_turn_directive(snapshot)
        │     │     ├── Proceed → continue
        │     │     └── FailFast → block (stall / replan cap)
        │     ├── Preload repair context (if validation failed)
        │     ├── Dispatch to execute_phase(phase)
        │     │     ├── Author: build prompt, tools, AgentCtx → agent loop
        │     │     ├── Plan: discovery + enrichment LLM calls
        │     │     ├── Validate: dbt compile/build
        │     │     ├── Review: batched LLM review
        │     │     └── Repair: dispatch to repair subroutine
        │     └── Return PhaseOutcome
        │
        ├── Runner handles outcome:
        │     ├── StayedWithProgress → reset budget, continue
        │     ├── StayedWaiting → increment idle counter
        │     ├── TransitionCommitted → clear idle counter
        │     ├── Return(frames) → stop, return frames
        │     └── Failed(reason) → stop, return error
        │
        ├── Budget exhaustion → executor.on_budget_exhausted()
        │
        └── Return Vec<FlowFrame>
```

#### Repair subroutine

When validation fails, the `data_engineer` suite enters repair mode. Instead of authoring new artifacts, it dispatches to a dedicated repair subroutine that uses its own `RepairExecutor` (also implementing `PhaseExecutor`) driven by the same `workflow::runner::run()`.

Each repair iteration runs four stages:

```
┌─────────────────────────────────────────────────────────────────┐
│                     Repair Iteration                             │
│                                                                  │
│  Gather (task_model)  ──►  Reason (reason_model)                │
│  Read SQL/YAML files       Produce JSON fix plan                │
│  Query vector store        (path, op, content)                  │
│  Diagnose root cause       No tools — pure reasoning            │
│                                                                  │
│  Apply (mechanical)   ──►  Validate (dbt build)                 │
│  Execute patch/write       Check if fixes resolved errors       │
│  Per-file escalation       Index results to vector store        │
│  (patch fail → write)                                           │
│                                                                  │
│  If validation passes → PhaseOutcome::Return (success)          │
│  If validation fails  → PhaseOutcome::TransitionCommitted       │
│                          (next iteration with accumulated        │
│                           history in RepairSessionLog)           │
└─────────────────────────────────────────────────────────────────┘
```

Key design principles:
- **Diagnosis-first**: The gather stage reads files and queries the vector store until confident of the root cause, before entering the fix stage.
- **History accumulation**: `RepairSessionLog` tracks all iterations (diagnosis, planned fixes, apply results, validation outcomes). The full history is passed to every subsequent LLM call, making it impossible for the LLM to repeat failed approaches.
- **Per-file escalation**: A failed `op=patch` on one file escalates to `op=write` for that file. Subsequent files still attempt `op=patch` first.
- **Vector store learning**: Repair outcomes (successes and failures) are indexed into the vector store, building institutional knowledge across runs.
- **Agnostic repair prompts**: No hardcoded error classification or fix nudges in prompts. The LLM diagnoses the issue independently based on file content and error context.

#### Transition intents

| Intent | Meaning | Effect |
|--------|---------|--------|
| `Forward` | Progress to next phase | Resets `replan_backtracks` to 0 |
| `Loopback` | Return to an earlier phase (e.g. plan revision) | May increment `replan_backtracks`; capped to prevent infinite loops |
| `Annotation` | Metadata-only update within the same phase | No backtrack counter change |

#### Guards

Guards are suite-owned. Core defines `PreTurnDirective<G>` generic over a guard kind type; the suite supplies its own `GuardBlockKind` enum via `WorkflowSuiteContract::GuardKind` and implements `guard_kind_as_str()` for serialization. `evaluate_pre_turn_directive` and `PreTurnStateSnapshot` live entirely in the suite (e.g. `data_engineer/phase_gate.rs`), not in core.

The `data_engineer` suite's `PreTurnDirective::FailFast` guards prevent runaway execution:
- **Stall in mutate mode** — author phase not making progress after mutations
- **Replan backtrack cap** — too many plan revisions (batch locked)

---

### Request lifecycle (end to end)

```
 Client          WS Server           Suite               Workflow Runner      Agent Loop
   │                │                   │                      │                  │
   │── user msg ───►│                   │                      │                  │
   │                │── build SuiteCtx  │                      │                  │
   │                │── resolve suite ─►│                      │                  │
   │                │   + agent_type    │                      │                  │
   │                │                   │── build executor     │                  │
   │                │                   │── workflow::run() ──►│                  │
   │                │                   │                      │── execute_turn() │
   │                │                   │                      │   ├─ load state  │
   │                │                   │                      │   ├─ pre-turn    │
   │                │                   │                      │   │  guard eval  │
   │                │                   │                      │   └─ phase ─────►│
   │                │                   │                      │                  │── LLM call
   │                │                   │                      │                  │── parse action
   │                │                   │                      │                  │── tool exec
   │                │                   │                      │                  │── observation
   │                │                   │                      │                  │── (repeat)
   │                │                   │                      │◄─ PhaseOutcome ──│
   │                │                   │                      │── budget/idle    │
   │                │                   │                      │   accounting     │
   │                │                   │                      │── (next turn     │
   │                │                   │                      │    or terminate) │
   │                │                   │◄─ Vec<FlowFrame> ───│                  │
   │                │◄─ Vec<FlowFrame> ─│                      │                  │
   │                │── map to ServerMsg│                      │                  │
   │◄─ WS response ─│                  │                      │                  │
   │                │── persist thread  │                      │                  │
```

---

### Extending the system

- **Add a new suite**: create `src/suites/react-suites/src/<your_suite>/` with a `Phase` enum, `domain_types` module (reason codes, guard kinds), control flow, prompts, and tools. Implement `PhaseExecutor` from `react-core::workflow` to define per-turn logic, and `WorkflowSuiteContract` for associated types. Register in `src/suites/react-suites/src/lib.rs` (`default_registry`). Define suite-specific provider traits in a `providers/` submodule, config types in a `<suite>_config.rs`, and capability accessors in a `ctx_ext.rs`. Use `workflow::runner::run()` as the main orchestration loop.
- **Add a sub-workflow** (e.g. a repair loop): implement `PhaseExecutor` for a new executor struct and call `workflow::runner::run()` from within an outer turn. The repair subroutine in `data_engineer` is a working example of this nested pattern.
- **Add a tool**: implement `Tool` and register it in the relevant phase's tool registry
- **Add a generic provider**: define a trait in `react-core` (`src/core/src/providers/`), implement it in a new module crate under `src/modules/`, wire it in the runtime
- **Add a suite-specific provider**: define the trait in the suite's `providers/` module, implement it in a module crate under the suite's `modules/` directory (e.g. `src/suites/react-suites/src/data_engineer/modules/`), register it as a capability via `set_capability()` in the runtime, and add an accessor function in the suite's `ctx_ext.rs`
- **Add an agent type**: define a new variant in the suite's `AgentMode` enum, select appropriate tools/policy/phases for it
- **Change completion semantics**: implement a new `AgentPolicy` and use it in the suite's `AgentCtx`
- **Swap infra**: construct a different `SuiteCtx` (different providers/storage/keyspace) and pass it to `ws::server::start_with_ctx`
