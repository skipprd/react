use serde_json::Value;
use std::sync::Arc;

use crate::capability::CapabilityMap;
use crate::keyspace::Keyspace;
use crate::provider_traits::VectorStore;
use crate::schema_registry::{AgentStepTypeV1, AgentStepV1, SchemaId};
use crate::scope::RequestScope;
use crate::session::{
    ExecutionContext, Observation, ThreadResult, ThreadStep, ThreadStore,
};
use crate::storage::StorageAdapter;
use crate::tools::ToolRegistry;
use async_trait::async_trait;

mod helpers;
mod llm_gateway;
mod parsing;
mod run_loop;

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CompleteEnvelope {
    pub kind: String,
    pub payload: Value,
    #[serde(default)]
    pub display: Option<String>,
}

#[derive(Clone)]
pub struct AgentCtx {
    top_k: usize,
    per_step_timeout_secs: u64,
    max_steps: usize,
    thread_id: Option<String>,
    progress_tx: Option<tokio::sync::mpsc::UnboundedSender<usize>>,
    pre_step_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    trace_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    agent_name: Option<String>,
    policy: Arc<dyn AgentPolicy>,
    llm: Arc<dyn crate::llm::LargeLanguageModel>,
    storage: Arc<dyn StorageAdapter>,
    scope: RequestScope,
    keyspace: Arc<dyn Keyspace>,
    vector: Option<Arc<dyn VectorStore>>,
    thread_store: Option<ThreadStore>,
    exec_ctx: Option<ExecutionContext>,
    resolved_config: Option<Arc<crate::resolved_config::ReactResolvedConfig>>,
    capabilities: CapabilityMap,
}

impl std::fmt::Debug for AgentCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AgentCtx { .. }")
    }
}

impl AgentCtx {
    // ── Accessors ──────────────────────────────────────────────
    pub fn top_k(&self) -> usize { self.top_k }
    pub fn per_step_timeout_secs(&self) -> u64 { self.per_step_timeout_secs }
    pub fn max_steps(&self) -> usize { self.max_steps }
    pub fn thread_id(&self) -> &Option<String> { &self.thread_id }
    pub fn progress_tx(&self) -> &Option<tokio::sync::mpsc::UnboundedSender<usize>> { &self.progress_tx }
    pub fn pre_step_tx(&self) -> &Option<tokio::sync::mpsc::UnboundedSender<String>> { &self.pre_step_tx }
    pub fn trace_tx(&self) -> &Option<tokio::sync::mpsc::UnboundedSender<String>> { &self.trace_tx }
    pub fn agent_name(&self) -> &Option<String> { &self.agent_name }
    pub fn policy(&self) -> &Arc<dyn AgentPolicy> { &self.policy }
    pub fn llm(&self) -> &Arc<dyn crate::llm::LargeLanguageModel> { &self.llm }
    pub fn storage(&self) -> &Arc<dyn StorageAdapter> { &self.storage }
    pub fn scope(&self) -> &RequestScope { &self.scope }
    pub fn keyspace(&self) -> &Arc<dyn Keyspace> { &self.keyspace }
    pub fn vector(&self) -> &Option<Arc<dyn VectorStore>> { &self.vector }
    pub fn thread_store(&self) -> &Option<ThreadStore> { &self.thread_store }
    pub fn exec_ctx(&self) -> &Option<ExecutionContext> { &self.exec_ctx }
    pub fn resolved_config(&self) -> &Option<Arc<crate::resolved_config::ReactResolvedConfig>> { &self.resolved_config }
    pub fn capabilities_ref(&self) -> &CapabilityMap { &self.capabilities }

    // ── Setters ────────────────────────────────────────────────
    pub fn set_policy(&mut self, v: Arc<dyn AgentPolicy>) { self.policy = v; }
    pub fn set_thread_id(&mut self, v: Option<String>) { self.thread_id = v; }
    pub fn set_thread_store(&mut self, v: Option<ThreadStore>) { self.thread_store = v; }
    pub fn set_exec_ctx(&mut self, v: Option<ExecutionContext>) { self.exec_ctx = v; }
    pub fn set_resolved_config(&mut self, v: Option<Arc<crate::resolved_config::ReactResolvedConfig>>) { self.resolved_config = v; }
    pub fn set_progress_tx(&mut self, v: Option<tokio::sync::mpsc::UnboundedSender<usize>>) { self.progress_tx = v; }
    pub fn set_pre_step_tx(&mut self, v: Option<tokio::sync::mpsc::UnboundedSender<String>>) { self.pre_step_tx = v; }

    /// Retrieve a suite-specific capability by concrete type.
    pub fn capability<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.capabilities.get::<T>()
    }

    /// Store a suite-specific capability by concrete type.
    pub fn set_capability<T: Send + Sync + 'static>(&mut self, val: Arc<T>) {
        self.capabilities.set(val);
    }

    pub fn agent_name_or_default(&self) -> String {
        self.agent_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
    }
}

// ── Builder ────────────────────────────────────────────────────
pub struct AgentCtxBuilder {
    top_k: usize,
    per_step_timeout_secs: u64,
    max_steps: usize,
    thread_id: Option<String>,
    progress_tx: Option<tokio::sync::mpsc::UnboundedSender<usize>>,
    pre_step_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    trace_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    agent_name: Option<String>,
    policy: Arc<dyn AgentPolicy>,
    llm: Arc<dyn crate::llm::LargeLanguageModel>,
    storage: Arc<dyn StorageAdapter>,
    scope: RequestScope,
    keyspace: Arc<dyn Keyspace>,
    vector: Option<Arc<dyn VectorStore>>,
    thread_store: Option<ThreadStore>,
    exec_ctx: Option<ExecutionContext>,
    resolved_config: Option<Arc<crate::resolved_config::ReactResolvedConfig>>,
    capabilities: CapabilityMap,
}

impl AgentCtxBuilder {
    pub fn new(
        llm: Arc<dyn crate::llm::LargeLanguageModel>,
        storage: Arc<dyn StorageAdapter>,
        scope: RequestScope,
        keyspace: Arc<dyn Keyspace>,
        policy: Arc<dyn AgentPolicy>,
    ) -> Self {
        Self {
            top_k: 10,
            per_step_timeout_secs: 30,
            max_steps: 10,
            thread_id: None,
            progress_tx: None,
            pre_step_tx: None,
            trace_tx: None,
            agent_name: None,
            policy,
            llm,
            storage,
            scope,
            keyspace,
            vector: None,
            thread_store: None,
            exec_ctx: None,
            resolved_config: None,
            capabilities: CapabilityMap::default(),
        }
    }

    pub fn top_k(mut self, v: usize) -> Self { self.top_k = v; self }
    pub fn per_step_timeout_secs(mut self, v: u64) -> Self { self.per_step_timeout_secs = v; self }
    pub fn max_steps(mut self, v: usize) -> Self { self.max_steps = v; self }
    pub fn thread_id(mut self, v: impl Into<String>) -> Self { self.thread_id = Some(v.into()); self }
    pub fn progress_tx(mut self, v: tokio::sync::mpsc::UnboundedSender<usize>) -> Self { self.progress_tx = Some(v); self }
    pub fn pre_step_tx(mut self, v: tokio::sync::mpsc::UnboundedSender<String>) -> Self { self.pre_step_tx = Some(v); self }
    pub fn trace_tx(mut self, v: Option<tokio::sync::mpsc::UnboundedSender<String>>) -> Self { self.trace_tx = v; self }
    pub fn agent_name(mut self, v: impl Into<String>) -> Self { self.agent_name = Some(v.into()); self }
    pub fn vector(mut self, v: Option<Arc<dyn VectorStore>>) -> Self { self.vector = v; self }
    pub fn thread_store(mut self, v: ThreadStore) -> Self { self.thread_store = Some(v); self }
    pub fn exec_ctx(mut self, v: ExecutionContext) -> Self { self.exec_ctx = Some(v); self }
    pub fn resolved_config(mut self, v: Option<Arc<crate::resolved_config::ReactResolvedConfig>>) -> Self { self.resolved_config = v; self }
    pub fn capabilities(mut self, v: CapabilityMap) -> Self { self.capabilities = v; self }

    pub fn build(self) -> AgentCtx {
        AgentCtx {
            top_k: self.top_k,
            per_step_timeout_secs: self.per_step_timeout_secs,
            max_steps: self.max_steps,
            thread_id: self.thread_id,
            progress_tx: self.progress_tx,
            pre_step_tx: self.pre_step_tx,
            trace_tx: self.trace_tx,
            agent_name: self.agent_name,
            policy: self.policy,
            llm: self.llm,
            storage: self.storage,
            scope: self.scope,
            keyspace: self.keyspace,
            vector: self.vector,
            thread_store: self.thread_store,
            exec_ctx: self.exec_ctx,
            resolved_config: self.resolved_config,
            capabilities: self.capabilities,
        }
    }
}

pub struct Agent;

#[derive(Clone, Debug)]
pub(crate) enum ParsedStep {
    Tool { name: String, args: Value },
    Complete { complete_env: CompleteEnvelope },
}

pub enum RunOutcome {
    Complete {
        thread_id: String,
        result: ThreadResult,
    },
    Interrupt {
        thread_id: String,
        kind: InterruptKind,
        prompt: String,
    },
}

pub enum CompleteDecision {
    Accept { result: ThreadResult },
    Reject { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InterruptKind {
    AwaitUser,
    AwaitApproval,
}

impl InterruptKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AwaitUser => "await_user",
            Self::AwaitApproval => "await_approval",
        }
    }
}

pub enum RunOutcomeNonInteractive {
    Complete {
        thread_id: String,
        result: ThreadResult,
    },
    StepBoundary {
        thread_id: String,
        reason: StepBoundaryReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepBoundaryReason {
    StepBudgetExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunLoopStop {
    RejectedComplete { reason: String },
    StepLimitExceeded,
    PolicyBlocked { reason: String },
}

#[async_trait]
pub trait AgentPolicy: Send + Sync {
    /// Extra transcript lines to inject after system/tool-card and before the user question.
    fn prelude_lines(
        &self,
        _ctx: &AgentCtx,
        _store: Option<&ThreadStore>,
        _thread_id: &str,
    ) -> Vec<String> {
        Vec::new()
    }

    /// Optional interrupt hook: after a tool action executes, policy may convert it into a control
    /// flow interrupt. Returns `(InterruptKind, prompt)`. Non-interactive mode suppresses these.
    fn interrupt_for_action(
        &self,
        _action_name: &str,
        _args: &Value,
        _obs: &Value,
    ) -> Option<(InterruptKind, String)> {
        None
    }

    /// Optional per-tool timeout override (in seconds).
    ///
    /// By default, tool calls are bounded by `AgentCtx.per_step_timeout_secs`. Suites can raise the
    /// timeout for known-slow tools (e.g. external data sources or LLM-backed generators) without
    /// globally increasing the timeout for every tool.
    fn timeout_for_tool(&self, _action_name: &str) -> Option<u64> {
        None
    }

    /// Human-readable name for a tool call, used in `ThreadStep::ToolStart.clean_name`.
    /// Override in suites that have domain-specific tool names.
    fn clean_tool_name(&self, name: &str, _args: &Value) -> String {
        helpers::title_case_words(&name.replace('_', " "))
    }

    /// Handle a model-emitted complete step. Return:
    /// - `Ok(CompleteDecision::Accept(..))` to accept and finish
    /// - `Ok(CompleteDecision::Reject { .. })` to reject and continue
    async fn handle_complete(
        &self,
        tools: &ToolRegistry,
        ctx: &AgentCtx,
        transcript: &mut Vec<String>,
        store: Option<&ThreadStore>,
        thread_id: &str,
        complete_env: &CompleteEnvelope,
    ) -> Result<CompleteDecision, String>;

    /// If we exhaust steps without reaching an accepted completion, produce a fallback.
    async fn fallback(
        &self,
        _tools: &ToolRegistry,
        _ctx: &AgentCtx,
        _transcript: &mut Vec<String>,
        _store: Option<&ThreadStore>,
        thread_id: &str,
    ) -> Result<RunOutcome, String> {
        Ok(RunOutcome::Interrupt {
            thread_id: thread_id.to_string(),
            kind: InterruptKind::AwaitUser,
            prompt: "Agent reached step limit without producing a valid completion. Please retry."
                .to_string(),
        })
    }
}

/// Default policy: accept any well-formed typed complete envelope.
pub struct DefaultPolicy;

#[async_trait]
impl AgentPolicy for DefaultPolicy {
    async fn handle_complete(
        &self,
        _tools: &ToolRegistry,
        ctx: &AgentCtx,
        _transcript: &mut Vec<String>,
        store: Option<&ThreadStore>,
        thread_id: &str,
        complete_env: &CompleteEnvelope,
    ) -> Result<CompleteDecision, String> {
        let result = ThreadResult {
            kind: complete_env.kind.clone(),
            payload: complete_env.payload.clone(),
            display: complete_env.display.clone(),
        };
        if let Some(store) = store {
            let agent = ctx.agent_name_or_default();
            let ts = chrono::Utc::now().to_rfc3339();
            let _ = store
                .append_step(
                    thread_id,
                    ThreadStep::Complete {
                        kind: result.kind.clone(),
                        payload: result.payload.clone(),
                        display: result.display.clone(),
                        observation: Observation::ok(),
                        ts,
                        agent,
                    },
                )
                .await;
        }
        Ok(CompleteDecision::Accept { result })
    }
}

/// Adapter policy for strict non-interactive runs.
///
/// It preserves all policy behavior except interactive interrupts/fallbacks.
/// This lets suites reuse existing policies while hard-cutting `AwaitUser`/`AwaitApproval`.
pub struct NonInteractivePolicyAdapter {
    pub inner: Arc<dyn AgentPolicy>,
}

#[async_trait]
impl AgentPolicy for NonInteractivePolicyAdapter {
    fn prelude_lines(
        &self,
        ctx: &AgentCtx,
        store: Option<&ThreadStore>,
        thread_id: &str,
    ) -> Vec<String> {
        self.inner.prelude_lines(ctx, store, thread_id)
    }

    fn interrupt_for_action(
        &self,
        _action_name: &str,
        _args: &Value,
        _obs: &Value,
    ) -> Option<(InterruptKind, String)> {
        None
    }

    fn timeout_for_tool(&self, action_name: &str) -> Option<u64> {
        self.inner.timeout_for_tool(action_name)
    }

    fn clean_tool_name(&self, name: &str, args: &Value) -> String {
        self.inner.clean_tool_name(name, args)
    }

    async fn handle_complete(
        &self,
        tools: &ToolRegistry,
        ctx: &AgentCtx,
        transcript: &mut Vec<String>,
        store: Option<&ThreadStore>,
        thread_id: &str,
        complete_env: &CompleteEnvelope,
    ) -> Result<CompleteDecision, String> {
        self.inner
            .handle_complete(tools, ctx, transcript, store, thread_id, complete_env)
            .await
    }

    async fn fallback(
        &self,
        tools: &ToolRegistry,
        ctx: &AgentCtx,
        transcript: &mut Vec<String>,
        store: Option<&ThreadStore>,
        thread_id: &str,
    ) -> Result<RunOutcome, String> {
        self.inner
            .fallback(tools, ctx, transcript, store, thread_id)
            .await
    }
}


#[cfg(test)]
mod tests;
