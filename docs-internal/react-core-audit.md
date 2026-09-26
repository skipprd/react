
# Skippr ReAct core audit: prioritized refactor list

Scope: `react` workspace at `main@6b821c9` (react-core, react-transport, react-view, react runtime, in-repo suites). This was a read-only audit. Paths below are relative to `src/`. `P0` means correctness or termination is broken today, `P1` means a design flaw that will bite long-running suites, and `P2` means cleanup.

## 0. Grounding: build, test, and repro results

| Check | Result |
|---|---|
| `cargo build` (pinned `rust-toolchain.toml` = 1.88.0) | **FAILS**: `lance@7.0.0`/`lancedb@0.30.0` in `Cargo.lock` require rustc 1.91. |
| `cargo build --workspace --exclude react-module-provider-vector-lance` (1.88) | OK |
| `cargo +1.91.0 build --workspace` | OK. Needs system `libssl-dev` (lance → reqwest → native-tls). |
| `cargo +1.91.0 clippy --workspace` | OK: 0 errors, 161 warnings (transport 114, mostly generated `api_gen`; runtime 38; core 4; kb 4; debugger 1) |
| `cargo +1.91.0 check --workspace --all-features` | **FAILS**: 10 errors, because `llama_cpp` feature code imports the undeclared crate `llama_cpp_2` (V-2) |
| `cargo +1.91.0 test --workspace --no-fail-fast` | **160 passed, 3 FAILED** (`react-transport` `ws::server::tests::plans_request_*`) |
| CI (`.github/workflows/ci.yml`) | The last 4 runs on `main` were all **cancelled after 24h** (self-hosted runners). `main` is effectively unverified. |

Reproductions were written as a scratch crate outside the repo. It depends on the workspace crates by absolute path `/workspace/src/...`, so adjust the paths in `Cargo.toml` to run it elsewhere. A copy is kept at `internal/react-core-audit-repro/` in this project store; run it with `cargo +1.88.0 test -- --test-threads=1`. **All 8 reproductions confirmed the suspected bugs:**

| Repro | Confirms |
|---|---|
| `llm_chat_json_panics_on_valid_json_wrong_shape` | P0 `S-2`: panic in `AgentCtx::llm_chat_json` |
| `thread_store_lost_update_across_instances` | P0 `L-1`: audit-log steps silently overwritten |
| `newest_observation_is_dropped_when_over_budget` | P0 `C-1`: model never sees its latest tool result |
| `heavy_prelude_drops_user_question` | P1 `C-2`: user question truncated out of prompt |
| `read_run_log_panics_on_utf8_boundary` | P0 `S-7`: tool panics on non-ASCII logs |
| `tool_output_without_ok_key_is_failure` | P1 `T-4`: success classified as failure by convention |
| `resilient_parse_prefers_last_object` | P2 `S-2`: first/last object inconsistency |
| `headless_auto_approve_reruns_without_bound` | P0 `B-1`: **489 suite re-entries in 3 s**, never terminates |

### 0.1 Tooling notes

- **Test failures:** `plans_request_returns_latest_plan_and_model_when_present`, `plans_request_includes_checklist_and_omits_notes`, and `plans_request_surfaces_parse_error_snapshot_for_corrupt_plan_json` (`transport/src/ws/server_tests.rs:581-802`). These tests write plans to storage and expect `KbSuite` to surface them, but plan loading moved behind `Suite::load_ws_plans` (default `Ok(vec![])`, `core/src/suite.rs:441-447`) when the product crates were moved out (`042ab99`). The tests are stale. Stalled CI hid this.
- **Memory:** linking the full test graph (lance + datafusion + aws-sdk) peaked around 10 GB RSS with `CARGO_BUILD_JOBS=3` on a 4-core / 15 GB VM. A cold build takes about 7 min, and the full test build about 12 min.
- **Clippy hotspots in hand-written code:** `too_many_arguments` at `core/src/agent/llm_gateway.rs:342`, `transport/src/ws/suite_runner.rs:45,115`, and `ws/terminal.rs:1417`. These point at the same design issue as D-1, D-2, and S-4 (parameter lists instead of request structs).

---

## 1. Target type spine (shared by all items below)

Most items converge on the same small set of types. They are defined once here and referenced by ID below.

```rust
// react-core/src/ids.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadId(uuid::Uuid);            // parsed once at the edge
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(uuid::Uuid);               // one per suite invocation
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Seq(u64);                        // monotonic per thread
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolName(&'static str);          // only constructible from Tool::NAME

// react-core/src/budget.rs  — one meter, consumed everywhere (see B-2)
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RunBudget {
    pub max_llm_calls: NonZeroU32,
    pub max_tool_calls: NonZeroU32,
    pub max_prompt_tokens_total: NonZeroU64,
    pub max_wall: Duration,
    pub max_identical_failures: NonZeroU8,     // see B-3
    pub max_auto_resumes: u8,                  // see B-1
}
pub struct BudgetMeter { /* atomics + Instant */ }
impl BudgetMeter {
    pub fn charge(&self, c: Charge) -> Result<(), BudgetExhausted>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetExhausted { LlmCalls, ToolCalls, PromptTokens, WallClock, IdenticalFailures, AutoResumes }

// react-core/src/mode.rs — see M-1
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode { Chat, Ask, Agent, Review }

// react-core/src/llm.rs — see S-1
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("fatal: {0}")]            Fatal(ProviderMessage),      // quota/auth/billing
    #[error("transient: {msg}")]      Transient { msg: ProviderMessage, retry_after: Option<Duration> },
    #[error("timeout after {0:?}")]   Timeout(Duration),
    #[error("protocol: {0}")]         Protocol(String),
    #[error("budget: {0:?}")]         Budget(BudgetExhausted),
}
```

---

## 2. Types and state machine

### T-1 · P0 · The transcript is `Vec<String>` sniffed by prefix, sent as a single user message
- `core/src/agent/run_loop.rs:153-169`, `run_loop.rs:25-46`, `agent/helpers.rs:34-113`, `run_loop.rs:84-94`
- Roles are encoded as `"System:"`/`"Tools:"`/`"User:"`/`"Observation:"` prefixes and found again with `starts_with`. The entire transcript, system prompt included, is sent as **one `ChatRole::User` message**, so role semantics and provider prompt caching are lost.

```rust
// current
Self::transcript_add(&mut transcript, format!("System: {system_prompt}"), &ctx.trace_tx);
let head_end = transcript.iter().position(|l| l.starts_with("User:"))...;
let messages = vec![ChatMessage { role: ChatRole::User, content: prompt }];
```
```rust
// proposed: react-core/src/agent/transcript.rs
pub enum Entry {
    System(Arc<str>), ToolCard(Arc<str>), Prelude(String), User(String),
    Assistant(AgentStep),                                  // typed, not raw text
    Observation { tool: ToolName, outcome: ToolOutcome },  // see T-4
    ParseRejected(StepParseError),                         // see S-2
    CompletionRejected { reason: String },
}
pub struct Transcript { head: Vec<Entry>, tail: VecDeque<Entry>, ledger: ErrorLedger }
impl Transcript {
    pub fn render(&self, budget: PromptBudget) -> Vec<ChatMessage>; // drops oldest tail first; never head
}
```

### T-2 · P1 · The wire step is converted to the internal step with ad-hoc `Option` checks
- `core/src/agent/parsing.rs:57-110`, `schema_registry.rs:53-69`
- The OpenAI strict-schema constraint forces a flat `AgentStepV1`, but the conversion is hand-written and lenient: `complete` steps that carry `name`/`args` are silently "stripped" (`parsing.rs:81-88`).

```rust
// proposed
pub enum AgentStep { Tool { name: String, args: Value }, Complete(CompleteEnvelope) }
impl TryFrom<AgentStepV1> for AgentStep {
    type Error = StepParseError;                   // enum, see S-2
    fn try_from(w: AgentStepV1) -> Result<Self, StepParseError> {
        match (w.type_, w.name, w.args, w.complete) {
            (AgentStepTypeV1::Tool, Some(n), Some(a), None)       => Ok(Self::Tool { name: n, args: parse_args(&a)? }),
            (AgentStepTypeV1::Complete, None, None, Some(c))      => Ok(Self::Complete(c.try_into()?)),
            (t, n, a, c) => Err(StepParseError::ShapeMismatch { ty: t, has_name: n.is_some(), has_args: a.is_some(), has_complete: c.is_some() }),
        }
    }
}
```

### T-3 · P1 · Five overlapping outcome enums, plus a hand-maintained delegation adapter
- `core/src/agent/mod.rs:311-364` (`RunOutcome`, `RunOutcomeNonInteractive`, `StepBoundaryReason`, `RunLoopStop`, `CompleteDecision`), `mod.rs:479-537` (`NonInteractivePolicyAdapter`), `run_loop.rs:96-130`
- The adapter must mirror every `AgentPolicy` method by hand. A new defaulted method silently bypasses `inner`. `run_until_block_non_interactive` maps *every* interrupt to `StepBudgetExhausted` (lossy). `RunLoopStop::RejectedComplete` is logged as a "stop" even though the loop continues (`run_loop.rs:207-218`).

```rust
// proposed: interactivity is a type parameter, so illegal interrupts don't compile
pub trait Interactivity { type Interrupt; }
pub enum Interactive {}    impl Interactivity for Interactive    { type Interrupt = Interrupt; }
pub enum NonInteractive {} impl Interactivity for NonInteractive { type Interrupt = std::convert::Infallible; }

pub enum LoopExit<I: Interactivity> {
    Completed(ThreadResult),
    Interrupted(I::Interrupt),
    Stopped(StopReason),          // StepLimit | Budget(BudgetExhausted) | RepeatedFailure{..} | Cancelled
}
pub async fn run_agent<I: Interactivity>(spec: &AgentSpec<'_>, ctx: &AgentCtx) -> Result<LoopExit<I>, AgentError>;
```

### T-4 · P1 · Tool success is a JSON-key convention; `ok: bool` + `errors: Vec` admits illegal states
- `core/src/tools/mod.rs:7-11`, `session/types.rs:292-447` (`normalize` defaults `ok` to **false** at `:416`, injects `"no error details were captured"` at `:436-438`), `agent/run_loop.rs:298-320`
- Two failure channels exist (`Err(String)` and `Ok({"ok":false})`). A tool returning `{"rows":[...]}` without `"ok": true` is recorded as failed (repro confirmed). `Observation{ok:true, errors:[..]}` is representable.

```rust
// current
#[async_trait] pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String>;
}
```
```rust
// proposed
#[async_trait]
pub trait Tool: Send + Sync + 'static {
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    const MAX_OUTPUT_BYTES: usize = 16 * 1024;            // bounded by construction (C-4)
    type Args: DeserializeOwned + JsonSchema + Send;
    type Output: Serialize + Send;
    async fn call(&self, args: Self::Args, ctx: &ToolCtx<'_>) -> Result<Self::Output, ToolError>;
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolError { pub class: ToolErrorClass, pub message: BoundedText<2048>, pub detail: Option<BlobRef> }
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ToolErrorClass { InvalidArgs, NotFound, Denied, Timeout, Upstream, Internal }
pub enum ToolOutcome { Ok(Value), Failed(ToolError) }    // replaces ToolObservation{ok,errors,..}
// ToolRegistry erases via a blanket `impl<T: Tool> DynTool for T` and derives the tool card from NAME/DESCRIPTION/schema_for!(Args).
```

### T-5 · P1 · `ThreadStep` repeats `ts/agent/observation` on 17 variants, with stringly kinds
- `core/src/session/types.rs:449-662`
- `ts: String` (RFC 3339 re-parsed downstream), `Phase.phase: String`, `reason_code: Option<String>`, `GuardBlock.kind: String`, `Interrupt.kind: String`, `Complete.kind: String`. There is no `seq`, `run_id`, or causal parent, and a macro exists only to extract `ts`.

```rust
// proposed
#[derive(Serialize, Deserialize)]
pub struct StepRecord {
    pub seq: Seq, pub run_id: RunId, pub parent: Option<Seq>,
    pub ts: chrono::DateTime<chrono::Utc>, pub actor: Actor, pub body: StepBody,
}
#[derive(Serialize, Deserialize)] #[serde(tag = "type", rename_all = "snake_case")]
pub enum StepBody {
    UserMessage { text: String },
    Decision { request: Seq, decision: ApprovalDecision },            // M-2
    ToolStart { call: ToolCallId, tool: String, args: Value },
    ToolEnd   { call: ToolCallId, outcome: ToolOutcome },             // T-4
    LlmCall   { call: LlmCallId, prompt: BlobRef, response: BlobRef, usage: Option<Usage>, error: Option<LlmErrorKind> },
    Phase     { to: PhaseName, from: Option<PhaseName>, reason: TransitionReason },
    Interrupt { kind: InterruptKind, prompt: String },
    Complete  { result: ThreadResult },
    Stopped   { reason: StopReason },
    Suite(SuiteEvent),                                                // opaque, schema-versioned
}
```

### T-6 · P1 · `ThreadEvent` is a flat struct with 12 `Option`s covering 4 kinds
- `core/src/session/types.rs:33-60`, `view/src/projection.rs:49-167`

```rust
// proposed
#[serde(tag = "event_kind", rename_all = "snake_case")]
pub enum ThreadEvent {
    ToolStart { step: Seq, tool_id: ToolCallId, name: String, clean_name: String, phase: PhaseName },
    ToolEnd   { step: Seq, tool_id: ToolCallId, name: String, status: EndStatus, error: Option<String>, phase: PhaseName },
    LlmStart  { step: Seq, call_id: LlmCallId, model: String, phase: PhaseName },
    LlmEnd    { step: Seq, call_id: LlmCallId, status: EndStatus, error: Option<String> },
}
```

### T-7 · P2 · Config structs admit illegal combinations; `create_llm` ignores its argument
- `core/src/resolved_config.rs:147-153` (`mode` + `Option<bucket>` + `Option<path>` + `Option<creds>`), `runtime/src/llm/mod.rs:5-8` (`LlmProviderType` duplicates `LlmProvider`), `runtime/src/llm/mod.rs:38-40` (`create_llm(_cfg)` always builds `RouterModel::new()` from globals)

```rust
pub enum StorageResolved { Local { root: PathBuf }, S3 { bucket: String, creds: S3Credentials } }
pub fn create_llm(cfg: &LlmResolved) -> Arc<dyn LargeLanguageModel>;   // actually uses cfg; no process globals
```

### T-8 · P2 · `thread_id: String` is re-validated at every entry point
- `transport/src/ws/handlers.rs:130-135, 279-284, 333-338, 384-390`, `ws/server.rs:289-295`, `ws/protocol.rs:127-131`
- Fix: `ThreadId` (§1) in `api::*Request` via serde; delete the checks.

### T-9 · P1 · Capabilities are looked up by `TypeId` at runtime, so missing deps surface mid-run
- `core/src/capability.rs:5-20`, `suite.rs:193-201`, `agent/mod.rs:153-161`, `suites/kb/src/lib.rs:140-142` (`"vector provider missing"` returned from inside the executor)

```rust
// proposed: suites declare typed deps; host must supply them to register
pub trait Suite: Send + Sync + 'static {
    type Deps: Send + Sync + 'static;                   // e.g. struct KbDeps { vector: Arc<dyn VectorStore> }
    ...
}
impl SuiteRegistry { pub fn register<S: Suite>(&mut self, s: S, deps: S::Deps) -> Result<(), DuplicateSuiteId>; }
```

---

## 3. Modes (chat / ask / agent / review)

### M-1 · P0 · Modes are strings end to end, typed only at the wire edge
- `transport/src/ws/conn_state.rs:188-204` (typed `api::AgentType` → `String`), `core/src/suite.rs:425-431, 449-471` (`agent_type: &str`), `suites/kb/src/lib.rs:125-133` (`if agent_type != "kb"`), `transport/src/ws/util.rs:5`
- The wire enum mixes modes (`Ask`, `Agent`) with suite ids (`Kb`, `Review`). An unsupported mode is caught only at runtime, inside the suite.

```rust
// current
fn supported_agent_types(&self) -> Vec<String> { vec!["ask".to_string()] }
async fn handle_new(&self, thread_id: &str, question: &str, agent_type: &str, ctx: &SuiteCtx) -> Result<Vec<FlowFrame>, String>;
```
```rust
// proposed
pub trait Suite {
    const ID: SuiteId;
    const MODES: &'static [Mode];                    // catalog derived from this
    const DEFAULT_MODE: Mode;
    async fn handle(&self, req: SuiteRequest<'_>, ctx: &SuiteCtx) -> Result<SuiteTurn, SuiteError>;
}
pub struct SuiteRequest<'a> { pub thread: ThreadId, pub run: RunId, pub mode: Mode, pub input: &'a UserInput, pub budget: &'a BudgetMeter }
// Transport rejects `mode ∉ S::MODES` before any suite code runs.
```

### M-2 · P0 · There is no chat mode: every turn starts a fresh transcript, and approve/reject are magic strings
- `core/src/agent/run_loop.rs:153-169` (transcript built from `system + tools + prelude + question` only), `suites/kb/src/lib.rs:242-262` (`handle_user` = `run_kb(text)`), `transport/src/ws/handlers.rs:350-366, 403-419` (`User{text:"approve"}` / `"reject"`, then the suite is called with `"Continue."`), `suite_runner.rs:706-728`
- A follow-up message ("now filter by 2024") reaches the model with no prior Q/A. Approve and reject reach the suite as the *same* input (`"Continue."`); the suite can distinguish them only by string-scanning the log.

```rust
// proposed
#[derive(Serialize, Deserialize)] #[serde(tag = "kind", rename_all = "snake_case")]
pub enum UserInput { Message { text: String }, Decision { request: Seq, decision: ApprovalDecision }, Resume }
#[derive(Clone, Copy, Serialize, Deserialize)] pub enum ApprovalDecision { Approve, Reject }

// Chat memory owned by core, bounded, built from the audit log (not suite prelude hacks)
pub struct ConversationWindow { turns: VecDeque<Turn>, max_turns: NonZeroU8, max_tokens: NonZeroU32 }
impl ConversationWindow {
    pub fn from_log(log: &[StepRecord], limits: WindowLimits) -> Self;   // last N user/assistant pairs + rolling summary
}
```

### M-3 · P1 · There is no thread-level state machine, so actions are accepted in any state
- `transport/src/ws/handlers.rs:325-428` (approve/reject accepted with no pending request), `conn_state.rs:108-168` (state recovered from 4 sources: memory maps, control state, log scan, defaults)

```rust
#[derive(Serialize, Deserialize)] #[serde(tag = "state", rename_all = "snake_case")]
pub enum ThreadState {
    Idle,
    Running { run: RunId, mode: Mode },
    AwaitingUser { run: RunId, prompt: String },
    AwaitingApproval { run: RunId, request: Seq, prompt: String },
    Completed { run: RunId },
    Failed { run: RunId, reason: StopReason },
}
impl ThreadState {
    pub fn accept(self, input: &UserInput) -> Result<ThreadState, InvalidTransition>; // Decision only from AwaitingApproval
}
```

### M-4 · P1 · `InterruptKind` round-trips through strings
- `suites/kb/src/lib.rs:99-104`, `suites/goggles_review/src/lib.rs:94-99`, `suites/suite_debugger/src/lib.rs` (same block), `core/src/suite.rs:69-72` (`Interrupt{kind: FlowKind(String)}`), `transport/src/ws/suite_runner.rs:576-582` (`if kind == "await_approval"`), `suite_runner.rs:575` (`Checkpoint` silently mapped to `Review`)

```rust
pub enum FlowFrame { Complete(ThreadResult), Review(Review), Checkpoint(Checkpoint), Interrupt { kind: InterruptKind, prompt: String } }
```

### M-5 · P2 · Duplicate decision enums; interrupt policy is chosen by env plus a magic `cid`
- `core/src/interrupt.rs:3-31` (`InterruptDecision` ≡ `ReviewDecision`), `transport/src/ws/suite_runner.rs:62-80` (`cid == "headless"`, `SKIPPR_EXECUTION_SURFACE == "ide_chat"`)

```rust
pub enum Decision { Forward, AutoApprove, Reject { reason: String } }
pub trait InterruptPolicy: Send + Sync { fn decide(&self, kind: InterruptKind, prompt: &str) -> Decision; }
pub enum Surface { Ws, Headless { auto_approve: bool }, IdeChat }     // passed in by the host, not read from env
```

---

## 4. Termination and budgets

### B-1 · P0 · Unbounded auto-approve / auto-resume loop (repro: 489 re-entries in 3 s)
- `transport/src/ws/suite_runner.rs:586-763` (`rerun = Some((User, "Continue."))` → `spawn_task` → `continue`, no counter), `suite_runner.rs:706-720` (each iteration also appends a `User{"approve"}` step, so the log grows without bound)

```rust
// current
if let Some((k2, q2)) = rerun { agent_task = spawn_task(k2, q2); continue; }
```
```rust
// proposed
let mut resumes: u8 = 0;
...
if let Some(next) = rerun {
    resumes = resumes.checked_add(1).filter(|n| *n <= budget.max_auto_resumes)
        .ok_or(StopReason::Budget(BudgetExhausted::AutoResumes))?;
    agent_task = spawn_task(next);
    continue;
}
```

### B-2 · P0 · There is no run-level budget, and retries multiply across layers
- HTTP retries: `runtime/src/llm/router.rs:188-208` (up to 10). Gateway: `core/src/agent/llm_gateway.rs:81-82, 127-201` (+2). Parse retries: `run_loop.rs:357-418` (+2 LLM calls per step, not counted in `max_steps`). Steps: `AgentCtx.max_steps`. Workflow: `workflow/runner.rs:16-23` (`max_total_steps: 500`, and `StayedWithProgress` resets the per-phase budget at `:83-86`).
- The worst case per workflow is ≈ 500 × max_steps × 3 × 3 × 4 HTTP calls. There is no token, cost, or wall-clock ceiling, and no cancellation.

```rust
// proposed: one meter threaded through AgentCtx; every LLM/tool call charges it
impl AgentCtx { pub fn budget(&self) -> &Arc<BudgetMeter>; pub fn cancel(&self) -> &tokio_util::sync::CancellationToken; }
async fn invoke_llm(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
    self.budget().charge(Charge::LlmCall { est_prompt_tokens: req.est_tokens() }).map_err(LlmError::Budget)?;
    tokio::select! { r = self.llm.chat(req) => r, _ = self.cancel().cancelled() => Err(LlmError::Cancelled) }
}
```

### B-3 · P1 · No repeated-action detection; the model forgets older failures after about 9 steps
- `core/src/agent/helpers.rs:37-39` (tail = 18 lines = 9 tool steps), `run_loop.rs:224-248`
- The same `(tool, args)` can fail identically until `max_steps`. The failure detail is also subject to C-1.

```rust
pub struct ActionLedger { seen: HashMap<Fingerprint, FailureCount>, cap: NonZeroU8 }   // bounded: max_steps entries
#[derive(Hash, Eq, PartialEq)] pub struct Fingerprint { tool: ToolName, args_hash: u64, error_class: ToolErrorClass }
impl ActionLedger {
    pub fn record(&mut self, fp: Fingerprint) -> Result<FailureCount, StopReason>; // Err(RepeatedFailure{fp, n}) at cap
}
```

### B-4 · P1 · The workflow runner drops frames, only clamps backtracks, and returns `String` errors
- `core/src/workflow/runner.rs:121` (`Return(frames)` discards the accumulated `out_frames`), `runner.rs:125-131` (the budget-exhaustion frames are also discarded), `runner.rs:88-90` (`StayedWaiting` does `remaining_steps += 1`, so waiting never consumes budget), `workflow/mod.rs:45-62` (`next_replan_backtracks` saturates at `cap` instead of failing), `runner.rs:74-77, 129-131` (`"headless_budget_exhausted: ..."` strings)

```rust
pub enum WorkflowError { HardCeiling { total: u32 }, PhaseBudget { phase: &'static str }, Stuck { waits: u32, last: String }, BacktrackCap { cap: u16 }, Failed(SuiteError), Budget(BudgetExhausted) }
pub fn next_replan_backtracks(cur: u16, intent: TransitionIntent, is_backtrack: bool, cap: NonZeroU16) -> Result<u16, WorkflowError>;
pub async fn run(exec: &dyn PhaseExecutor, cfg: &Config) -> Result<SuiteTurn, (WorkflowError, Vec<FlowFrame>)>;
```

### B-5 · P1 · LLM calls and suite tasks cannot be cancelled, so resources leak on timeout or disconnect
- `core/src/agent/llm_gateway.rs:92-111` (`spawn_blocking` + `tokio::time::timeout`: on timeout, the blocking thread keeps running), `runtime/src/llm/router.rs:211-266, 385` (a `Condvar` inflight permit is held by the abandoned thread, and the HTTP timeout is up to 1800 s at `:190-198`), `transport/src/ws/suite_runner.rs:138-155` (the `tokio::spawn`ed suite task is detached and not aborted when the socket closes)

```rust
#[async_trait] pub trait LargeLanguageModel: Send + Sync {
    async fn chat(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError>;   // async, cancel-safe
}
struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);
impl<T> Drop for AbortOnDrop<T> { fn drop(&mut self) { self.0.abort() } }
```

### B-6 · P2 · The LLM memo cache ignores format, temperature, and schema, and defeats resampling
- `runtime/src/llm/router.rs:150-180, 310-327` (key = `model | hash(messages)`, TTL 120 s)
- An adversarial loop that re-asks the same prompt for an independent sample gets the cached answer, and two calls that differ only in `response_format` share one response.
- Fix: delete the memo, or key on `hash(serde_json::to_vec(&full_request))` and make it opt-in per `LlmCallOptions { cache: CachePolicy::Reuse }`.

### B-7 · P1 · The typed workflow contract exists but nothing drives it
- `core/src/suite.rs:474-500` (`WorkflowSuiteContract`, `WorkflowNodeContract`), `workflow/mod.rs:13-80` (`PhaseDirective`, `TransitionIntent`, `evaluate_pre_turn`, `reduce_event`, `phase_from_state`). There are zero consumers in this repo. All three in-repo suites run `max_phase_steps: 1` (`suites/kb/src/lib.rs:164-169`, etc.).
- Choose one of two options. (a) Delete it. (b) Make the runner generic over it so transitions and backtrack caps are enforced by core. (b) is recommended if `sde` uses it (verify downstream first).

```rust
pub trait Workflow: Send + Sync {
    type Phase: Copy + Eq + Serialize + DeserializeOwned + strum::IntoStaticStr + 'static;
    type State: Serialize + DeserializeOwned + Send;
    const START: Self::Phase;
    const TERMINAL: &'static [Self::Phase];
    fn is_backtrack(from: Self::Phase, to: Self::Phase) -> bool;
    async fn step(&self, st: &mut Self::State, at: Self::Phase, ctx: &StepCtx<'_>) -> Transition<Self::Phase>;
}
pub enum Transition<P> { Stay(Progress), Goto { to: P, reason: TransitionReason }, Done(ThreadResult), Block(GuardBlock) }
pub async fn drive<W: Workflow>(w: &W, st: &mut W::State, cfg: &Config, meter: &BudgetMeter) -> Result<ThreadResult, WorkflowError>;
```

---

## 5. Context and token bounding

### C-1 · P0 · When over budget, the newest observation is popped, permanently (repro confirmed)
- `core/src/agent/helpers.rs:104-111` (`transcript.pop()` removes the *latest* line, and the transcript is mutated in place), `helpers.rs:37-39` (24 head + 18 tail × 2500 chars ≈ 105 k chars, against a default 32 k budget from `error_context.rs:9`), so this path is hit routinely

```rust
// current
if transcript.len() <= 4 { return ...; }
transcript.pop();                                   // drops the result the model just asked for
```
```rust
// proposed: render into a fresh Vec; never mutate history; evict oldest tail first
pub fn render(&self, b: PromptBudget) -> Vec<ChatMessage> {
    let mut out = self.head_messages();              // never evicted
    let mut used = token_estimate(&out);
    let mut tail: Vec<ChatMessage> = Vec::new();
    for e in self.tail.iter().rev() {                // newest first
        let m = e.render(b.per_entry);
        let t = token_estimate_one(&m);
        if used + t > b.total { break; }
        used += t; tail.push(m);
    }
    debug_assert!(used <= b.total);
    out.extend(tail.into_iter().rev()); out
}
```

### C-2 · P1 · More than 22 prelude lines truncates the user question (repro confirmed)
- `core/src/agent/helpers.rs:50-60` (`head.truncate(MAX_HEAD_LINES)` runs after the head is taken *through* the first `User:` line)
- Fix: this falls out of T-1 (the `Entry::User` is structurally part of the head). Budget the prelude separately as `PromptBudget.prelude`.

### C-3 · P1 · Three conflicting truncation layers; the budget comes from env, not config
- `core/src/agent/helpers.rs:39` (`MAX_LINE_CHARS = 2_500`), `error_context.rs:7-10` (8 k / 32 k), `error_context.rs:32-52` (`LLM_MAX_PROMPT_CHARS` / `LLM_CONTEXT_LENGTH` env), which ignores `LlmResolved.context_length` (`resolved_config.rs:162`). `run_loop.rs:341-354` computes a keyword excerpt for the *remaining* budget, and then `helpers.rs:42-48` head-truncates it to 2500 chars, which cuts away the excerpt's keyword windows.

```rust
#[derive(Clone, Copy, Debug)]
pub struct PromptBudget { pub total: NonZeroU32, pub per_entry: NonZeroU32, pub prelude: u32, pub ledger: u32 } // tokens
impl PromptBudget { pub fn from_model(ctx_len: NonZeroU32, max_output: NonZeroU32) -> Self; }  // single source
```

### C-4 · P1 · Tool outputs and LLM responses are unbounded in memory and in the audit log
- `session/types.rs:487-503` (`ToolEnd.observation` holds the full output), `types.rs:532-552` (`LlmCall.response_text` holds the full response), `suites/suite_debugger/src/tools/read_log.rs:35-38` (default 50 000 chars returned, then cut to 2 500 by the transcript)
- Fix: `Tool::MAX_OUTPUT_BYTES` (T-4), enforced in the registry's erased `call`. Outputs above the cap are stored as a `BlobRef` (content-addressed object in storage) and summarized in the step.

```rust
pub struct BoundedText<const N: usize>(String);          // constructor truncates on char boundary + marks
pub struct BlobRef { pub sha256: [u8; 32], pub bytes: u64 }
```

### C-5 · P1 · No structured error memory across steps, runs, or rounds
- Parse errors (`run_loop.rs:376-389`), completion rejections (policy-written free text), and tool failures (`run_loop.rs:341-354`) are free-text lines that fall out of the 18-line tail.

```rust
pub struct ErrorLedger { entries: IndexMap<Fingerprint, LedgerEntry>, cap: NonZeroU8 }   // LRU by last_seen
pub struct LedgerEntry { first_seen: Seq, last_seen: Seq, count: u16, excerpt: BoundedText<400> }
impl ErrorLedger { pub fn render(&self, tokens: u32) -> ChatMessage; }  // fixed-size "Known failures" section in head
```

### C-6 · P2 · Hot paths load the full log (O(n) per call, O(n²) per run)
- `core/src/agent/llm_gateway.rs:224-243` (every LLM call loads the whole thread log to find the current phase), `store_io.rs:166-193` (`append_step_if_new` scans the log), `transport/src/ws/suite_runner.rs:322-345` (polls the full log every 200 ms, 250 ms, and 800 ms; `phase_runs_from_steps(&log.steps[..=i])` per phase event is O(n²)), `suites/kb/src/lib.rs:114-121` (`step_count` loads the entire log)

```rust
pub trait ThreadLogRead { async fn read_since(&self, t: ThreadId, after: Seq, limit: NonZeroU16) -> CoreResult<Vec<StepRecord>>; async fn head(&self, t: ThreadId) -> CoreResult<LogHead /* seq, phase, state */>; }
// transport subscribes to an in-process broadcast of StepRecord instead of polling.
```

### C-7 · P2 · The thread list loads every log in full, with no pagination
- `transport/src/ws/protocol.rs:40-103`
- Fix: persist `LogHead { title, last_ts, preview: BoundedText<200>, suite, mode }` next to the log, and page with `ListRequest { after: Option<ThreadId>, limit: NonZeroU16 }`.

---

## 6. Audit log

### L-1 · P0 · Lost update: a fresh etag is paired with a stale cached body (repro confirmed)
- `core/src/session/store_io.rs:78-102` (`head_etag` is fresh, but the body comes from the per-instance `cache` for up to 5 s), `session/mod.rs:43-50` (each `ThreadStore::new` has its own cache), `suite.rs:172-178` and `conn_state.rs:58-71` (a new store per call). `AgentCtx.thread_store` is long-lived, while `SuiteCtx::log_writer()` creates new instances, so interleaved writes overwrite each other. `storage.rs:55-194` (`CachingStorageAdapter`) is an unbounded, never-evicted byte cache with the same staleness problem.

```rust
// current
let current_etag = retry_head_etag(...).await?;
let log = if let Some(entry) = self.cache.get(key) { if fresh { entry.log.clone() } ... };
```
```rust
// minimal fix (PR 1)
pub(crate) struct CacheEntry { pub etag: String, pub log: ThreadLog, pub ts: Instant }
let log = match self.cache.get(key) { Some(e) if e.etag == etag => e.log.clone(), _ => reload().await? };
// structural fix (PR 6): append-only segments, see L-3; no read-modify-write of a whole document.
```

### L-2 · P0 · Audit writes are fire-and-forget
- **17 of 21** production `append_step` call sites discard the `Result` with `let _ =` (e.g. `agent/mod.rs:457`, `run_loop.rs:70`, `llm_gateway.rs:289, 320, 374`, `suite.rs:219`, `store_io.rs:191`, `transport/src/ws/handlers.rs:37, 49, 92, 146, 164, 242, 300, 351, 404`, `suite_runner.rs:709`). The other 4 only log a warning (`session/observed.rs:87, 134, 225`). An etag conflict returns `Err` with no retry (`store_io.rs:118-124`).

```rust
#[must_use = "audit writes must be handled"]
pub async fn append(&self, t: ThreadId, body: StepBody, actor: Actor) -> Result<Seq, AuditError>;
pub enum AuditError { Conflict { retries: u8 }, Storage(StorageError), TooLarge { bytes: u64, cap: u64 } }
// append retries Conflict up to AUDIT_MAX_CONFLICT_RETRIES (const, e.g. 5); a failure stops the run with StopReason::AuditUnavailable.
```

### L-3 · P1 · Every append rewrites the entire thread document
- `core/src/session/store_io.rs:127-164` (read → push → serialize the whole `ThreadLog` → conditional put). Bytes written are O(n²) over a thread's life, and the document size has no bound. A long adversarial loop hits object-size, latency, and memory ceilings.

```rust
// proposed layout: threads/{id}/head.json  +  threads/{id}/seg/{first_seq:020}.jsonl  (≤ SEGMENT_MAX_STEPS lines)
pub const SEGMENT_MAX_STEPS: u32 = 256;
pub const THREAD_MAX_STEPS: u64 = 1_000_000;   // hard assert; runs stop with StopReason::AuditFull
```

### L-4 · P1 · Identifiers are not unique or causal
- `core/src/llm_observability.rs:134-170` (`call_id` comes from a process-global map that is **cleared at 10 000 entries** and restarts at 1 in every process, so call ids repeat within a thread), `runtime/src/llm/router.rs:392` (the router allocates a *second* `call_id` for the same call), `session/types.rs:449-632` (no `seq`, `run_id`, or parent)
- Fix: `StepRecord { seq, run_id, parent }` (T-5), plus `LlmCallId(Seq)` = the seq of the `LlmCall` step. Delete the global counters.

### L-5 · P1 · Not replayable: prompts are off by default, and dedup points at process memory
- `core/src/llm_observability.rs:8-14` (prompt parts are recorded only if `REACT_LOG_LLM_CALLS`/`REACT_LOG_THREAD_STEPS` is set), `llm_observability.rs:137-152, 186-207` (`"unchanged: <hash>"` references a part emitted in an earlier `LlmCall` whose append may have been dropped, see L-2), `run_loop.rs:207-218` (a rejected completion is recorded as `RunLoopStop`)
- Fix: always write `prompt: BlobRef` / `response: BlobRef` (content-addressed, so dedup is free and durable), and write the blob **before** the step. Replay is `fold(StepRecord) -> ThreadState`, and prompts are re-rendered from blobs.

### L-6 · P1 · The completion has two writers, deduplicated by payload equality
- `core/src/agent/mod.rs:454-470` (`DefaultPolicy::handle_complete` appends `Complete`), `suite.rs:205-223` (`record_flow_frames` appends it again via `append_step_if_new`), `store_io.rs:166-193` (duplicates detected by comparing `(kind, payload)` across the whole log)
- Fix: policies return decisions only. The loop owner writes exactly one `StepBody::Complete`.

### L-7 · P2 · The headless outcome is derived from opaque suite JSON and error substrings
- `transport/src/headless.rs:281-317` (`control.load::<Value>()` then `.get("phase_state").get("mode") == "failed"`), `headless.rs:16-39` (`classify_error` matches suite-specific text such as `"invalid staging model sql"` and `"batch_locked"`), `headless.rs:230` (`Lagged(_) => continue` can drop `Final`, which yields exit 2)
- Fix: core persists `ThreadState` (M-3), and exit codes are derived from it:

```rust
#[repr(i32)] pub enum ExitCode { Ok = 0, Failed = 1, Indeterminate = 2, Interrupted = 130 }
impl From<&ThreadState> for ExitCode { ... }
```

---

## 7. String parsing (replace with types)

### S-1 · P0 · LLM errors travel as `Ok(String)` with `LLM_ERROR:` / `LLM_FATAL_ERROR:` prefixes
- `runtime/src/llm/types.rs:33-63, 77-81` (errors encoded into response text), `core/src/agent/llm_gateway.rs:114-121, 134-147, 195-199`, `run_loop.rs:181-184`, `core/src/llm.rs:147-165` (`is_throttle`: 16 substrings; `"timeout"` also matches non-transient errors, e.g. a 400 about a `timeout` parameter), `runtime/src/llm/router.rs:700-704, 755-759` (substring checks for transient errors), `llm_gateway.rs:81-82, 171`

```rust
// current
fn chat(&self, messages: &[ChatMessage], options: &LlmCallOptions) -> Result<String, String>;
if trimmed.starts_with("LLM_ERROR:") || trimmed.starts_with("LLM_FATAL_ERROR:") { ... }
```
```rust
// proposed (LlmError in §1)
async fn chat(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError>;
pub struct LlmResponse { pub text: String, pub usage: Option<Usage>, pub finish: FinishReason }
pub enum FinishReason { Stop, Length, ContentFilter, ToolCall }
// classification happens once, in the provider adapter, from HTTP status + provider error `code`.
impl LlmError { pub fn retry(&self) -> RetryClass { match self { Self::Transient{..} | Self::Timeout(_) => RetryClass::Backoff, _ => RetryClass::Never } } }
```

### S-2 · P0 · Parse-retry classification uses `Display` prefixes, and one path panics (repro confirmed)
- `core/src/agent/run_loop.rs:16-23` (`s.starts_with("invalid JSON from model:") || s.contains("validation error:") ...`), `run_loop.rs:371-375` (`err_label` via `contains`), `llm_gateway.rs:410` (`serde_json::from_str::<Value>(&raw).unwrap_err()` **panics** when the model returns valid JSON of the wrong shape), `parsing.rs:26-31` (the same unwrap_err pattern), `json_repair.rs:221-226` (returns the *last* embedded object; its doc says first)

```rust
#[derive(Debug, thiserror::Error, Clone)]
pub enum StepParseError {
    #[error("not JSON: {0}")]            NotJson(String),
    #[error("schema: {0}")]              Schema(String),
    #[error("shape: {ty:?}")]            ShapeMismatch { ty: AgentStepTypeV1, has_name: bool, has_args: bool, has_complete: bool },
    #[error("args not JSON: {0}")]       Args(String),
}
impl StepParseError { pub const fn retriable(&self) -> bool { true } pub const fn label(&self) -> &'static str { /* match */ } }
pub fn resilient_parse<T: DeserializeOwned>(s: &str) -> Result<T, StepParseError>;   // never panics; picks first object
```

### S-3 · P1 · Storage errors are classified by substring; "not found" is an error string
- `core/src/storage.rs:196-253` (`"os error 2"`, `"not found"`, `"status code: 404"`), `storage.rs:243-252` (retries `Unknown`, including deserialization errors, 6× ≈ 6.3 s), `storage.rs:274-365` (parses AWS `Debug` output)

```rust
#[async_trait] pub trait StorageAdapter: Send + Sync {
    async fn get(&self, key: &Key) -> Result<Option<Versioned<Bytes>>, StorageError>;   // NotFound = Ok(None)
    async fn put_if(&self, key: &Key, body: Bytes, cond: WriteCondition) -> Result<Written, StorageError>;
    async fn list(&self, prefix: &Key, page: Page) -> Result<Listing, StorageError>;
    async fn delete(&self, key: &Key) -> Result<(), StorageError>;
}
pub enum StorageError { Conflict { current: Option<Etag> }, AuthExpired, Throttled, Timeout, Corrupt(String), Other(String) }
pub enum WriteCondition { Always, IfAbsent, IfMatch(Etag) }
```

### S-4 · P0 · The WS protocol dispatches on `v["type"]` strings; headless round-trips through JSON
- `transport/src/ws/server.rs:47-155` (5 `if t == "new" | "open" | ...` branches, each with a copy-pasted 15-line error block), `ws/protocol.rs:23-37` (a second string `match`), `ws/server.rs:302-347` (headless builds `json!({...})` and the handlers parse it back; `question != "continue" && question != "go"` at `:306`), `server.rs:231-241` (`CaptureSink` re-parses every outgoing JSON to find `thread_assigned`), `conn_state.rs:81-90` (`buffer_last` re-parses JSON to read `seq`)

```rust
#[derive(Deserialize)] #[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage { New(api::NewRequest), Open(api::OpenRequest), User(api::UserRequest), Approve(api::ApproveRequest),
                         Reject(api::RejectRequest), List(api::ListRequest), Suites(api::SuitesRequest), History(api::HistoryRequest),
                         Seen(api::SeenRequest), Plans(api::PlansRequest), ThreadState(api::ThreadStateRequest), Delete(api::DeleteRequest), Cancel(CancelRequest) }

async fn handle(msg: ClientMessage, st: &mut ConnState, out: &mut impl EventSink) -> Result<(), ProtocolError>;
pub trait EventSink { async fn send(&mut self, m: api::ServerMessage); }   // WS, hub, headless all implement; no JSON round-trip
pub async fn run_headless(ctx: SuiteCtx, req: HeadlessRequest) -> Result<ThreadId, RunError>;  // calls handle() directly
```

### S-5 · P1 · The final result's type is guessed from payload keys
- `transport/src/ws/mapping.rs:51-102` (`kind` is ignored; `answer` + `sql` ⇒ `Ask`, `answer` ⇒ `Kb`, else `Generic`), `mapping.rs:126-134` (plan keys read from an untyped map)
- Fix: a suite-declared result schema:

```rust
pub struct ThreadResult { pub kind: ResultKind, pub payload: Value, pub display: Option<String> }
#[derive(Serialize, Deserialize, Clone, Copy)] #[serde(rename_all = "snake_case")]
pub enum ResultKind { Ask, Kb, Generic }            // or `S::Result: Serialize + JsonSchema` via Suite assoc type
```

### S-6 · P1 · The OpenAI API is chosen by model-name prefix, in 4 places with inconsistent rules
- `runtime/src/llm/registry.rs:30-38` (`gpt-5` | `o4`), `openai_responses_adapter.rs:18` (`gpt-5` | `o`), `llm/session.rs:112` (`gpt-5` | `o`), `openai_compat.rs:132, 304` (`gpt-5` | `o4`)

```rust
#[derive(Deserialize, Clone, Copy)] #[serde(rename_all = "snake_case")]
pub enum OpenAiApi { Responses, ChatCompletions }
pub struct ModelSpec { pub id: String, pub api: OpenAiApi, pub background: bool, pub context_tokens: NonZeroU32 }  // from config
```

### S-7 · P0/P2 · The suite debugger infers inputs from free text, and one tool panics
- **P0** `suites/suite_debugger/src/tools/read_log.rs:39-40` (`&text[..max_len]` byte-slices UTF-8, which panics on any multibyte log; repro confirmed)
- P2 `suites/suite_debugger/src/lib.rs:277-285` (thread id = any hex-ish word ≥ 8 chars in the question), `lib.rs:287-296` (suite id via `lower.contains("kb")`), `lib.rs:298-311`
- Fix: `BoundedText::truncate_chars`, typed `Args { thread_id: ThreadId, max_chars: Option<NonZeroU32> }` (T-4), and read the suite from the target thread's `ThreadState`/`SwitchSuite` step.

### S-8 · P2 · Configuration is read from about 52 env vars and mutated with `set_var` at runtime
- `core/src/lib.rs:37-46` (`env_truthy`), `error_context.rs:32-52`, `llm_observability.rs:8-14`, `runtime/src/run_engine.rs:72-74, 219-232, 245-251` (`std::env::set_var("REACT_HEADLESS", "1")`, which is `unsafe` in edition 2024 and racy with concurrent readers), `suites/kb/src/lib.rs:52-70` (reasoning effort parsed from env with a hand-written `match` on lowercase strings)

```rust
#[derive(Deserialize)] #[serde(rename_all = "snake_case")] pub enum ReasoningEffort { None, Low, Medium, High, #[serde(alias = "xhigh")] ExtraHigh }
pub struct RuntimeConfig { pub llm: LlmResolved, pub prompt: PromptBudget, pub budget: RunBudget, pub observability: Observability, pub surface: Surface }
// passed down explicitly via SuiteCtx/AgentCtx; env is read exactly once in `config.rs`.
```

---

## 8. DRY and abstraction collapse

### D-1 · P1 · Three suites copy-paste the same executor (about 100 lines each)
- `suites/kb/src/lib.rs:17-122, 146-173, 198-262`, `suites/goggles_review/src/lib.rs:31-146`, `suites/suite_debugger/src/lib.rs:18-190`. Each one builds an `AgentCtx`, calls `run_until_block`, maps `RunOutcome` → `FlowFrame` (including the `InterruptKind` → string block), implements `step_count` by loading the full log, and has three identical `handle_*` bodies.

```rust
// proposed: react-core/src/suite/single_agent.rs
pub struct AgentSpec<'a> { pub name: &'static str, pub system: &'a str, pub tools: &'a ToolRegistry, pub policy: Arc<dyn AgentPolicy>,
                           pub llm: LlmCallOptions, pub max_steps: NonZeroU16, pub step_timeout: Duration }
pub async fn run_single_agent(spec: &AgentSpec<'_>, req: &SuiteRequest<'_>, ctx: &SuiteCtx) -> Result<SuiteTurn, SuiteError>;
// kb/goggles/debugger `handle` each become ~10 lines.
```

### D-2 · P1 · `handle_new`, `handle_open`, and `handle_user` have identical signatures
- `core/src/suite.rs:449-471`, `transport/src/ws/suite_runner.rs:39-43, 141-151`
- Fix: one `handle(SuiteRequest)` (M-1), with `enum Entry { New, Open, Input(UserInput) }` inside the request if a suite needs to distinguish them.

### D-3 · P2 · Single-impl traits, pure-delegation wrappers, and dead code
| Item | Location | Action |
|---|---|---|
| `ThreadLogReader` (1 impl) | `session/log_reader.rs:10-15` | Replace with a read-only newtype `ThreadLogView(ThreadStore)` |
| `ThreadLogWriter` (pure delegation) | `session/log_writer.rs:10-79` | Delete; `ThreadStore::append` |
| `StateStore` (0 impls; plumbed through `SuiteCtx` and its builder) | `provider_traits/state.rs`, `suite.rs:86,143,160,282,320` | Delete |
| `WorkflowSuiteContract`/`WorkflowNodeContract` + 3 one-line wrappers | `suite.rs:474-500`, `workflow/mod.rs:65-80` | B-7 (drive or delete) |
| `discover::stats` (domain profiling in core, unused) | `core/src/discover*` | Move to the data-engineer suite |
| `helpers::progress` (TTY spinner in core, unused) | `core/src/helpers/progress.rs` | Move to transport or delete |
| `runtime::llm::thread_ctx` (re-export wrapper) | `runtime/src/llm/thread_ctx.rs:1-12` | Delete |
| `OpenAICompatModel` (~550 lines, test-only; a parallel OpenAI stack) | `runtime/src/llm/openai_compat.rs:95-651` | Delete |
| `#[allow(dead_code)]` type aliases | `transport/src/ws/mapping.rs:15-30` | Delete |
| `ThreadStoreConfig.max_events` (never read) | `session/mod.rs:25, 32` | Delete |
| Hand-written builders + getters/setters (~250 lines) | `agent/mod.rs:61-301`, `suite.rs:96-343` | Public fields on a `#[non_exhaustive]` struct, or `bon`/`typed-builder` with required fields |
| `session_write_lock` static map grows one entry per thread, forever | `session/mod.rs:52-59` | Hold locks in `ThreadStore`, evict on drop (`Weak`) |

### D-4 · P2 · `LlmCallOptions.expected_format` is silently overwritten
- Suites set `JsonObject` (`suites/kb/src/lib.rs:74`, goggles `:63`, debugger `:320`), and `run_until_block` overwrites it (`agent/run_loop.rs:140-143`).
- Fix: `AgentSpec.llm: AgentLlmOptions` (no format field). `LlmCallOptions.expected_format` stays only for direct `llm_chat` calls.

### D-5 · P2 · Retry/backoff is implemented 4 times
- `core/src/storage.rs:367-423`, `agent/llm_gateway.rs:127-201`, `runtime/src/llm/router.rs:657-715, 717-770`

```rust
pub struct RetryPolicy { pub delays: &'static [Duration] }         // length = max attempts; const-constructible
pub async fn retry<T, E: Classify, F: FnMut() -> Fut, Fut: Future<Output = Result<T, E>>>(p: &RetryPolicy, meter: &BudgetMeter, f: F) -> Result<T, E>;
pub trait Classify { fn retry(&self) -> RetryClass; }
```

### D-6 · P2 · WS handlers duplicate the ack, switch, and user-append blocks 5 times
- `transport/src/ws/handlers.rs:19-428`
- Fix: `fn ack(cid) -> ServerMessage` and `async fn begin_run(st, thread, mode, input) -> Result<RunId, _>`, used by all entry points once S-4 lands.

### D-7 · P2 · Domain leakage into the generic runtime
- `suites/goggles_review` (bridging-loan prompts, `lib.rs:17-29`, despite `design.md` saying "Goggles-specific review code belongs to Goggles"), `transport/src/headless.rs:32-33` (`invalid staging model sql`), `ws/mapping.rs:51-84` (Ask = `sql` + `chart`), `ws/mapping.rs:126-134` (`plan_kind`/`workgroup_id` keys), `transport/src/ws/terminal_dbt.rs`
- Fix: move these to their product repos, and keep only generic `ResultKind::Generic` in core.

### D-8 · P2 · `jsonschema` pulls an HTTP client into `react-core`; the validator is rebuilt on every call
- `core/Cargo.toml` (`jsonschema = "0.42.0"` default features → reqwest + rustls compiled into core), `core/src/schema_registry.rs:230-242` (`validator_for` on every step)

```rust
static AGENT_STEP_VALIDATOR: LazyLock<jsonschema::Validator> = LazyLock::new(|| jsonschema::validator_for(&json_schema(SchemaId::AgentStepV1)).expect("static schema"));
// Cargo.toml: jsonschema = { version = "0.42", default-features = false }
```

---

## 9. Suite and client API for long-running adversarial loops

What core provides today: a single ReAct loop (`run_until_block`) with a trimmed transcript, a string-keyed `prelude_lines` hook, an opaque `ControlStateStore` (`Value` + `suite_id` string), a workflow runner that the in-repo suites use as a single-shot wrapper, and a polling-based WS stream.

What is missing for audit, plan, test, and verify loops that run for hours: a typed root of truth, a bounded memory of prior failures and findings, budgets, adversarial roles, and client control (cancel, resume, and cursor-based streaming).

### H-1 · P0 · There is no single root of truth, and control-state writes can be lost
- Truth is spread across four places: `ConnState.current_suite/current_agent` maps (`conn_state.rs:15-26`), `ControlStateStore` (opaque `Value`), `ThreadLog` scans (`conn_state.rs:131-152`), and the view cache.
- `control_state.rs:151-162` (`save` is an unconditional put), `control_state.rs:164-195` (`mutate` returns a conflict error with **no retry**), `control_state.rs:66-79` (a suite-id mismatch silently returns `None`)

```rust
pub struct ThreadRoot<S> { pub schema: SchemaVersion, pub suite: SuiteId, pub state: ThreadState, pub suite_state: S, pub head: Seq }
impl<S: Serialize + DeserializeOwned> ThreadRootStore<S> {
    pub async fn load(&self, t: ThreadId) -> Result<Option<Versioned<ThreadRoot<S>>>, StoreError>;
    pub async fn mutate<R>(&self, t: ThreadId, f: impl FnMut(&mut ThreadRoot<S>) -> Result<R, SuiteError>) -> Result<R, StoreError>; // bounded CAS retry
}
// Invariant (assert on load): root.head == last StepRecord.seq; ThreadState == fold(log).
```

### H-2 · P1 · No core primitives for adversarial loops that need bounded context
The proposal below keeps this to one generic loop plus two bounded data structures, with no trait hierarchy.

```rust
// react-core/src/rounds.rs
pub struct Finding { pub id: FindingId, pub severity: Severity, pub status: FindingStatus, pub claim: BoundedText<400>, pub evidence: Vec<BlobRef> /* ≤ 8 */ }
pub enum FindingStatus { Open, Fixed { round: u16 }, Rejected { by: Role, reason: BoundedText<200> }, Verified { round: u16 } }
pub enum Role { Proposer, Critic, Verifier }

/// Bounded, deduplicated, persisted in ThreadRoot.suite_state; rendered as a fixed token section.
pub struct Ledger { findings: IndexMap<FindingId, Finding>, cap: NonZeroU16 }

pub struct RoundSpec<'a> { pub proposer: AgentSpec<'a>, pub critic: AgentSpec<'a>, pub verifier: Option<AgentSpec<'a>>,
                           pub max_rounds: NonZeroU16, pub stop: StopRule, pub context: PromptBudget }
pub enum StopRule { NoOpenAbove(Severity), StableFor(NonZeroU8) /* rounds without new findings */ }

/// Each round sees: root truth (goal + accepted artifact ref), Ledger.render(), ErrorLedger.render(),
/// and only the previous round's diff — never the full history.
pub async fn run_rounds(spec: &RoundSpec<'_>, ledger: &mut Ledger, ctx: &SuiteCtx, meter: &BudgetMeter) -> Result<RoundsOutcome, WorkflowError>;
pub enum RoundsOutcome { Converged { rounds: u16 }, Exhausted(BudgetExhausted), MaxRounds, Stalled { rounds: u16 } }
```

Termination is guaranteed by `max_rounds` (a `NonZeroU16`), the `BudgetMeter`, and `StopRule::StableFor`. Context is bounded by `Ledger.cap × BoundedText` plus a fixed `PromptBudget`.

### H-3 · P1 · The client API has no cancel, no run identity, no resume cursor, and a blocking read loop
- `transport/src/ws/server.rs:42-192` (the connection processes one message at a time, so a running suite blocks `list` and every other message), `headless.rs:230` (`Lagged` drops events), `conn_state.rs:15-26` (`seq: i32` per connection; no durable cursor)

```rust
pub enum ClientMessage { ..., Cancel { thread: ThreadId, run: RunId }, Subscribe { thread: ThreadId, after: Seq } }
pub enum ServerMessage { ..., Step { thread: ThreadId, record: StepRecordView }, RunEnded { thread: ThreadId, run: RunId, outcome: StopReasonOrResult } }
// Runs execute in a per-thread actor (mpsc inbox, AbortOnDrop). The socket reader never awaits suite work.
```

### H-4 · P2 · Registries silently overwrite duplicates; the tool card drifts from the registry
- `core/src/suite.rs:521-524`, `tools/mod.rs:29-31` (`HashMap::insert`), `suites/goggles_review/src/lib.rs:28-29` (hand-written tool card)

```rust
pub fn register<T: Tool>(&mut self, t: T) -> Result<(), DuplicateTool>;
pub fn tool_card(&self) -> String;   // generated from NAME + DESCRIPTION + schema_for!(Args); the only card source
```

---

## 10. Build and CI

### V-1 · P0 · The pinned toolchain cannot build the locked dependency graph, and CI has not run
- `rust-toolchain.toml` (1.88.0) versus `Cargo.lock` (`lance 7.0.0`, `lancedb 0.30.0`, `roaring 0.11.4`, all requiring ≥ 1.90/1.91). The last 4 `main` CI runs were cancelled after 24 h. The environment also needs `libssl-dev` (lance → reqwest → native-tls).
- Fix: bump `channel = "1.91.0"` (verified: the full workspace builds with it), or `cargo update -p lancedb --precise <1.88-compatible>`. Add `rust-version` to the manifests, and add a GitHub-hosted fallback job so CI can't silently stall.

### V-2 · P0 · The `llama_cpp` feature does not compile (this violates AGENTS.md rule 1)
- `runtime/Cargo.toml:47` (`llama_cpp = []`: it enables no dependency), `runtime/src/llm/llama_cpp.rs:3-12, 129` (`use llama_cpp_2::...`). `cargo check --workspace --all-features` fails with 10 errors: E0433/E0432 `llama_cpp_2` unresolved.

```toml
# proposed
[dependencies]
llama-cpp-2 = { version = "…", optional = true }
[features]
llama_cpp = ["dep:llama-cpp-2"]
```
- Add `cargo check --workspace --all-features` to CI (after V-1), so feature-gated code is compiled on every PR.

---

## 11. Suggested PR sequencing

Each PR compiles and passes tests on its own. The order puts unblocking first, then P0 bug fixes with no API change, then type spines, then the structural moves.

| # | PR | Items | Notes |
|---|---|---|---|
| 1 | **Unblock CI**: toolchain 1.91, `rust-version`, fix the `llama_cpp` feature, `--all-features` CI job, fix or move the 3 stale `plans_request_*` tests, `jsonschema` default-features off, CI fallback runner | V-1, V-2, D-8 | Tiny; nothing else is verifiable without it |
| 2 | **P0 hotfixes (no public API change)** plus the 8 repro tests from `internal/react-core-audit-repro/` as regression tests | S-2 (panic), S-7 (UTF-8), C-1 (pop oldest), C-2 (head keeps User), L-1 (etag-checked cache), B-1 (`MAX_AUTO_RESUMES`), L-2 (retry conflicts; stop swallowing in core paths) | Keeps `sde` source-compatible |
| 3 | **Typed errors**: `LlmError`, `StepParseError`, `StorageError`, one `retry()` helper; delete prefix/substring classifiers and the memo cache or key it correctly | S-1, S-2, S-3, S-6, D-5, B-6 | Breaking for provider/adapters. `LargeLanguageModel` becomes async (B-5) in the same PR to avoid breaking it twice |
| 4 | **Budgets and termination**: `RunBudget`/`BudgetMeter`, `CancellationToken`, `ActionLedger`, `AbortOnDrop` suite tasks, typed `WorkflowError`, frames preserved | B-2, B-3, B-4, B-5 | Adds a required `budget` to `AgentCtx` construction (compile-time enforced) |
| 5 | **Transcript and context**: `Entry` transcript rendered to real `ChatMessage` roles, `PromptBudget` from config, `ErrorLedger`, `Tool` v2 with typed `Args` and `MAX_OUTPUT_BYTES`, `BlobRef` | T-1, T-2, T-4, C-3, C-4, C-5, H-4 | Tool v2 can ship with an erased-compat shim for one release |
| 6 | **Typed protocol and modes**: `ClientMessage`/`EventSink`, `ThreadId`, `Mode`, `UserInput`/`ApprovalDecision`, `InterruptKind` in `FlowFrame`, `Surface` | S-4, S-5, M-1, M-4, M-5, T-8, D-6 | Regenerate OpenAPI (`scripts/gen-openapi-core.sh`); the wire format can stay JSON-compatible |
| 7 | **Audit log v5**: `StepRecord{seq,run_id,parent}`, `StepBody`, append-only segments + `head.json`, single writer, blob-backed prompts, `read_since` + broadcast instead of polling | T-5, T-6, L-3, L-4, L-5, L-6, C-6, C-7 | Needs a migration reader for schema v4 logs |
| 8 | **Suite API v2 and root of truth**: `Suite::handle(SuiteRequest) -> SuiteTurn`, `type Deps`, `ThreadState` machine, `ThreadRoot<S>` with CAS retry, `ConversationWindow` chat mode, shared `run_single_agent`, typed headless `ExitCode` | M-2, M-3, H-1, T-9, D-1, D-2, D-4, L-7 | The largest downstream break. Land it with the `sde` migration branch |
| 9 | **Adversarial loop kit and client control**: `Ledger`, `RoundSpec`/`run_rounds`, `Cancel`/`Subscribe` messages, per-thread actor | H-2, H-3, B-7 | Depends on PRs 4, 5, 7, and 8 |
| 10 | **Deletions**: D-3 table, domain leakage moved out, config structs as enums | D-3, D-7, T-7, S-8 | Check `sde` usage of `WorkflowSuiteContract`, `llm_chat_json`, and `set_throttle_observer` before deleting |
