## ReAct (`react` crate)

A **WebSocket-based ReAct agent runtime**. Clients send JSON frames, the server routes each request to a **suite**, and the suite drives a **phase-based workflow** where each phase runs a **ReAct loop** (LLM → tool calls → observations → complete/interrupt), using injected providers (query/catalog/vector/dbt/storage).

### Getting started

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
├── src/core          (react-core)        Core loop, traits, session, types — no infra deps
├── src/runtime       (react)             CLI, WS server, concrete provider wiring
├── src/suites        (react-suites)      Suite implementations
│   └── lib.rs + one directory per suite (data_engineer/, kb/)
│
└── src/modules                           Pluggable provider implementations
    ├── storage                           S3 adapter
    ├── provider-athena                   Athena warehouse
    ├── provider-bigquery                 BigQuery warehouse
    ├── provider-postgres                 Postgres warehouse
    ├── provider-dbt                      dbt CLI wrapper
    └── provider-vector-lance             LanceDB vector store
```

**Dependency rule**: `react-core` has zero workspace dependencies. Everything depends inward on `react-core`. The runtime crate wires concrete modules into the core's trait-based abstractions. Suites are fully self-contained — they depend only on `react-core`, never on each other.

```
runtime ──► react-core ◄── react-suites
  │                             │
  └──► modules (storage,        └── (uses core traits only)
       providers)
```

---

### Layer-by-layer

#### 1. Transport — WebSocket server

**Crate**: `runtime` · **File**: `src/runtime/src/ws/server.rs`

The WS server is a thin routing layer. It never calls the agent loop directly.

- Parses inbound frames (`new`, `open`, `user`, `approve`, `delete`, etc.)
- Creates/loads threads and persists user steps
- Resolves `suite_id` + `agent_type` and dispatches to the suite
- Converts the suite's `FlowFrame` output into wire-format `ServerMessage`s and streams them back
- Constructs the `SuiteCtx` (injected capabilities) for each request via the runtime's provider wiring

**`FlowFrame` → wire mapping:**

| FlowFrame | ServerMessage |
|-----------|---------------|
| `Complete { kind, payload, display }` | `Final` |
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
| **Prompts** | System prompt and tool card per phase |
| **Tool registry** | Which tools are available in each phase |
| **Policy** | An `AgentPolicy` that gates completions and interrupts |
| **Planning LLM calls** | Pre-loop LLM work (design memos, enrichment, critique) via `AgentCtx::llm_chat` |
| **Phase transition logic** | Rules for advancing, looping back, or blocking |
| **Agent types** | Different modes of the same suite (e.g. interactive ask vs autonomous agent) |

Registered suites:
- **`data_engineer`** — multi-phase analytics + dbt workflow (plan → author → validate → review → publish)
- **`kb`** — knowledge-base Q&A

Suites return `Vec<FlowFrame>` to the transport:

```rust
enum FlowFrame {
    Complete   { kind, payload, display },   // terminal result
    Review     { text, meta },               // review checkpoint
    Checkpoint { kind, payload },            // internal checkpoint
    Interrupt  { kind, prompt },             // pause for user/approval
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
├── resolved_config  Parsed YAML config
├── query?           QueryProvider
├── datasets?        DatasetCatalogProvider
├── warehouse        WarehouseProvider
├── catalog?         CatalogProvider
├── vector?          VectorStore
├── dbt?             DbtProvider
├── state?           StateStore
└── trace_tx?        Trace channel
```

**`AgentCtx`** is what the suite builds *per phase* for the agent loop. It narrows `SuiteCtx` to what a single ReAct invocation needs:

```
AgentCtx
├── llm              LLM handle (from SuiteCtx)
├── policy           AgentPolicy (suite-chosen, phase-specific)
├── storage          StorageAdapter
├── scope / keyspace (from SuiteCtx)
├── query? / warehouse / dbt? / vector?   (subset of providers)
├── thread_store?    ThreadStore (for transcript persistence)
├── max_steps        Step limit for this phase
├── per_step_timeout_secs
├── resolved_config  (from SuiteCtx)
└── exec_ctx?        ExecutionContext (for hierarchical UI rendering)
```

The `SuiteCtx → AgentCtx` construction is where the suite sets the policy, step limits, and tool registry for each phase. The agent loop itself never sees `SuiteCtx`.

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

#### 5. Agent loop — ReAct runtime

**Crate**: `react-core` · **Files**: `src/core/src/agent/`

The agent loop is a generic ReAct executor. It has no knowledge of suites, phases, or domain logic — it only knows about tools, an LLM, and a policy.

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

#### 6. LLM gateway

**File**: `src/core/src/agent/llm_gateway.rs`

All LLM calls — whether from the agent loop or from suite planning code — go through a single entry point on `AgentCtx`:

| Method | Purpose |
|--------|---------|
| `llm_chat(messages, options)` | Raw chat completion. Emits `LlmStart`/`LlmEnd`/`LlmCall` thread events automatically. Supports per-call `timeout_secs`. |
| `llm_chat_json<T>(messages, options)` | Calls `llm_chat`, deserialises the response as JSON, auto-repairs malformed JSON (escape control chars), and retries once with a "return only JSON" nudge on failure. |
| `llm_embed(text)` | Embedding pass-through. |

This eliminates scattered observability code and ensures every LLM interaction appears in the thread timeline.

#### 7. Tools

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

#### 8. Policies

**Trait**: `AgentPolicy` in `src/core/src/agent/mod.rs`

Policies let suites control the agent loop's behaviour without modifying it:

| Method | When called | Purpose |
|--------|-------------|---------|
| `handle_complete(...)` | Model emits a Complete action | Accept → `Some(RunOutcome::Complete)`. Reject → `None` (appends an observation, loop continues). Enables validation gates — e.g. require successful `dbt_validate` before accepting. |
| `interrupt_for_action(...)` | After each tool call | Return `Some((InterruptKind, prompt))` to pause execution and hand control back to the user/approver. |
| `timeout_for_tool(name)` | Before tool execution | Per-tool timeout override. |
| `prelude_lines(...)` | Before prompt construction | Extra transcript lines injected before the user's question. |
| `fallback(...)` | Step limit reached | Default: interrupt with "step limit" message. |

Key policy patterns:
- **`InterruptOnlyPolicy`** — converts `ask_user`/`ask_approval` tool calls into interrupts; used in interactive modes.
- **`NonInteractivePolicyAdapter`** — wraps any policy and suppresses all interrupts; the agent never pauses in non-interactive mode (but the user can still inject context into a running thread).
- **`SqlValidatedPolicy`** — gates completion on successful SQL execution / dbt validation.

#### 9. Providers — capabilities injection

**Traits**: `src/core/src/providers/` · **Concrete impls**: `src/modules/` + `src/runtime/src/providers/`

Providers are trait objects injected via `SuiteCtx`. The core and suites never import concrete implementations.

| Trait | Capability |
|-------|------------|
| `QueryProvider` | SQL execution, schema introspection, sampling |
| `WarehouseProvider` | Warehouse metadata (databases, schemas, tables) |
| `DatasetCatalogProvider` | Dataset discovery and bootstrap |
| `CatalogProvider` | Semantic catalog (descriptions, lineage) |
| `VectorStore` | Embedding upsert/query |
| `DbtProvider` | dbt project scaffolding, validation, run |
| `StorageAdapter` | Key-value persistence (local FS or S3) |
| `Keyspace` | Scoped key/URI layout for persistence (tenant/workspace/project hierarchy) |
| `StateStore` | Execution state persistence |
| `SecretsProvider` | Credential resolution |

The runtime crate (`src/runtime/src/main.rs`) constructs the concrete provider set and injects it. Swapping Athena for BigQuery is a one-line config change — no suite or core code changes.

#### 10. Session and persistence

**Files**: `src/core/src/session/`

Every interaction is stored as a **thread** — an append-only sequence of `ThreadStep`s:

```
ThreadStep::User         — user message
ThreadStep::LlmStart     — LLM call began (prompt hash, call id)
ThreadStep::LlmEnd       — LLM call returned (call id, token count)
ThreadStep::LlmCall      — full LLM interaction (messages, response, metadata)
ThreadStep::ToolStart     — tool invoked (name, args)
ThreadStep::ToolEnd       — tool returned (name, result, duration)
ThreadStep::Phase         — phase transition
ThreadStep::Complete      — terminal result
```

`ThreadStore` persists steps as JSON via `StorageAdapter` + `Keyspace`. This gives full observability and replay capability.

---

### Phase-driven workflow

Suites orchestrate multi-phase workflows. Each phase is a node in a state machine; the suite's `control_flow` module defines valid transitions.

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

Not all agent types traverse the full graph. `ask` mode runs a single phase (no state machine). `review` mode runs only a review phase. The `agent` type drives the full plan → author → validate → review → publish pipeline.

#### How a phase executes

```
Suite.handle_user(thread_id, text, agent_type, ctx)
  │
  ▼
run_agent (phase loop)
  │
  ├── Load ExecutionState → current phase
  │
  ├── evaluate_pre_turn_directive(snapshot)
  │     ├── Proceed → continue
  │     └── FailFast → Block (stall / replan cap / repair ladder)
  │
  ├── execute_phase(phase)
  │     ├── Build system prompt + tool card for this phase
  │     ├── Register phase-appropriate tools
  │     ├── Construct AgentCtx with policy (SuiteCtx → AgentCtx)
  │     ├── Run Agent::run_until_block (the ReAct loop)
  │     └── Map RunOutcome to PhaseDirective
  │           ├── Transition { to, intent, reason }
  │           └── Block { kind, reason }
  │
  ├── dispatch_phase_transition(directive)
  │     ├── Validate transition is allowed
  │     ├── Update replan_backtracks on loopbacks
  │     ├── Persist ExecutionState + ThreadStep::Phase
  │     └── Return new phase
  │
  └── Loop until a phase returns FlowFrame(s)
```

#### Transition intents

| Intent | Meaning | Effect |
|--------|---------|--------|
| `Forward` | Progress to next phase | Resets `replan_backtracks` to 0 |
| `Loopback` | Return to an earlier phase (e.g. plan revision) | May increment `replan_backtracks`; capped to prevent infinite loops |
| `Annotation` | Metadata-only update within the same phase | No backtrack counter change |

#### Guards

`PreTurnDirective::FailFast` prevents runaway execution:
- **Stall in mutate mode** — author phase not making progress
- **Replan backtrack cap** — too many plan revisions (batch locked)
- **Repair ladder stop** — dbt repair attempts exhausted

---

### Request lifecycle (end to end)

```
 Client                WS Server              Suite                Agent Loop
   │                      │                      │                      │
   │── user message ─────►│                      │                      │
   │                      │── build SuiteCtx     │                      │
   │                      │── resolve suite ─────►│                      │
   │                      │   + agent_type       │                      │
   │                      │                      │── load phase state   │
   │                      │                      │── build AgentCtx     │
   │                      │                      │── execute_phase ────►│
   │                      │                      │                      │── LLM call
   │                      │                      │                      │── parse action
   │                      │                      │                      │── tool exec
   │                      │                      │                      │── observation
   │                      │                      │                      │── (repeat)
   │                      │                      │                      │── Complete/Interrupt
   │                      │                      │◄─ RunOutcome ────────│
   │                      │                      │── transition phase   │
   │                      │                      │── (next phase or     │
   │                      │                      │    return frames)    │
   │                      │◄─ Vec<FlowFrame> ────│                      │
   │                      │── map to ServerMsg   │                      │
   │◄─ WS response ──────│                      │                      │
   │                      │── persist thread     │                      │
```

---

### Extending the system

- **Add a new suite**: create `src/suites/react-suites/src/<your_suite>/` with a `Phase` enum, control flow, prompts, and tools. Register it in `src/suites/react-suites/src/lib.rs` (`default_registry`).
- **Add a tool**: implement `Tool` and register it in the relevant phase's tool registry
- **Add a provider**: define a trait in `react-core`, implement it in a new module crate, wire it in the runtime
- **Add an agent type**: define a new variant in the suite's `AgentMode` enum, select appropriate tools/policy/phases for it
- **Change completion semantics**: implement a new `AgentPolicy` and use it in the suite's `AgentCtx`
- **Swap infra**: construct a different `SuiteCtx` (different providers/storage/keyspace) and pass it to `ws::server::start_with_ctx`
