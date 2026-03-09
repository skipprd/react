use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::capability::CapabilityMap;
use crate::keyspace::{DefaultKeyspace, Keyspace};
use crate::llm::{DynLlm, NullModel};
use crate::provider_traits::{NullSecretsProvider, SecretsProvider, StateStore, VectorStore};
use crate::resolved_config::ReactResolvedConfig;
use crate::scope::RequestScope;
use crate::session::{ControlStateStore, Observation, ThreadLogReader, ThreadLogWriter, ThreadStep, ThreadStore};
use crate::error::CoreError;
use crate::storage::{ConditionalWriteStatus, StorageAdapter};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct FlowKind(pub String);

impl FlowKind {
    pub fn new(kind: impl Into<String>) -> Self {
        Self(kind.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for FlowKind {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for FlowKind {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Standardized suite output type.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FlowFrame {
    Complete {
        kind: FlowKind,
        payload: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<String>,
    },
    Review {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },
    /// Non-terminal checkpoint: the agent has produced intermediate results
    /// and the suite should continue with deterministic processing.
    Checkpoint {
        kind: FlowKind,
        payload: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<String>,
    },
    Interrupt {
        kind: FlowKind,
        prompt: String,
    },
}

/// Context passed to suites.
#[derive(Clone)]
pub struct SuiteCtx {
    storage: Arc<dyn StorageAdapter>,
    scope: RequestScope,
    keyspace: Arc<dyn Keyspace>,
    secrets: Arc<dyn SecretsProvider>,
    llm: DynLlm,
    resolved_config: Option<Arc<ReactResolvedConfig>>,
    trace_tx: Option<UnboundedSender<String>>,
    vector: Option<Arc<dyn VectorStore>>,
    state: Option<Arc<dyn StateStore>>,
    capabilities: CapabilityMap,
}

impl std::fmt::Debug for SuiteCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SuiteCtx { .. }")
    }
}

impl SuiteCtx {
    pub fn new(
        storage: Arc<dyn StorageAdapter>,
        secrets: Arc<dyn SecretsProvider>,
        llm: DynLlm,
        scope: RequestScope,
        keyspace: Arc<dyn Keyspace>,
    ) -> Self {
        Self {
            storage,
            scope,
            keyspace,
            secrets,
            llm,
            resolved_config: None,
            trace_tx: None,
            vector: None,
            state: None,
            capabilities: CapabilityMap::default(),
        }
    }

    // ── Accessors ──────────────────────────────────────────────
    pub fn storage(&self) -> &Arc<dyn StorageAdapter> { &self.storage }
    pub fn scope(&self) -> &RequestScope { &self.scope }
    pub fn keyspace(&self) -> &Arc<dyn Keyspace> { &self.keyspace }
    pub fn secrets(&self) -> &Arc<dyn SecretsProvider> { &self.secrets }
    pub fn llm(&self) -> &DynLlm { &self.llm }
    pub fn resolved_config(&self) -> &Option<Arc<ReactResolvedConfig>> { &self.resolved_config }
    pub fn trace_tx(&self) -> &Option<UnboundedSender<String>> { &self.trace_tx }
    pub fn vector(&self) -> &Option<Arc<dyn VectorStore>> { &self.vector }
    pub fn state(&self) -> &Option<Arc<dyn StateStore>> { &self.state }
    pub fn capabilities_ref(&self) -> &CapabilityMap { &self.capabilities }

    // ── Setters ────────────────────────────────────────────────
    pub fn set_resolved_config(&mut self, v: Option<Arc<ReactResolvedConfig>>) { self.resolved_config = v; }
    pub fn set_trace_tx(&mut self, v: Option<UnboundedSender<String>>) { self.trace_tx = v; }
    pub fn set_vector(&mut self, v: Option<Arc<dyn VectorStore>>) { self.vector = v; }
    pub fn set_state(&mut self, v: Option<Arc<dyn StateStore>>) { self.state = v; }

    pub fn control_store(&self) -> ControlStateStore {
        ControlStateStore::new(self.storage.clone(), self.scope.clone(), self.keyspace.clone())
    }

    pub fn log_writer(&self) -> ThreadLogWriter {
        ThreadLogWriter::new(self.storage.clone(), self.scope.clone(), self.keyspace.clone())
    }

    /// Read-only ThreadLog access for display/projection consumers.
    pub fn log_reader(&self) -> impl ThreadLogReader {
        ThreadStore::new(self.storage.clone(), self.scope.clone(), self.keyspace.clone())
    }

    pub fn llm_embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, crate::CoreError> {
        self.llm.embed(texts).map_err(crate::CoreError::generic)
    }

    /// Retrieve a suite-specific capability by concrete type.
    pub fn capability<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.capabilities.get::<T>()
    }

    /// Store a suite-specific capability by concrete type.
    pub fn set_capability<T: Send + Sync + 'static>(&mut self, val: Arc<T>) {
        self.capabilities.set(val);
    }

    /// Record `FlowFrame`s as `ThreadStep`s in the audit log before returning
    /// them to the transport. The suite owns writes; transport only observes.
    pub async fn record_flow_frames(&self, thread_id: &str, agent_type: &str, frames: &[FlowFrame]) {
        let writer = self.log_writer();
        for frame in frames {
            let step = flow_frame_to_step(frame, agent_type);
            match &step {
                ThreadStep::Complete { .. } | ThreadStep::Interrupt { .. } => {
                    writer.append_step_if_new(thread_id, step).await;
                }
                _ => {
                    let _ = writer.append_step(thread_id, step).await;
                }
            }
        }
    }
}

fn flow_frame_to_step(frame: &FlowFrame, agent_type: &str) -> ThreadStep {
    let ts = chrono::Utc::now().to_rfc3339();
    let agent = agent_type.to_string();
    let observation = Observation::ok();
    match frame {
        FlowFrame::Complete { kind, payload, display } => ThreadStep::Complete {
            kind: kind.0.clone(),
            payload: payload.clone(),
            display: display.clone(),
            observation,
            ts,
            agent,
        },
        FlowFrame::Review { text, meta } => ThreadStep::ReviewResponse {
            text: text.clone(),
            meta: meta.clone(),
            observation,
            ts,
            agent,
        },
        FlowFrame::Checkpoint { kind, payload, display } => ThreadStep::Checkpoint {
            kind: kind.0.clone(),
            payload: payload.clone(),
            display: display.clone(),
            observation,
            ts,
            agent,
        },
        FlowFrame::Interrupt { kind, prompt } => ThreadStep::Interrupt {
            kind: kind.0.clone(),
            prompt: prompt.clone(),
            observation,
            ts,
            agent,
        },
    }
}

// ── Builder ────────────────────────────────────────────────────
pub struct SuiteCtxBuilder {
    storage: Arc<dyn StorageAdapter>,
    scope: RequestScope,
    keyspace: Arc<dyn Keyspace>,
    secrets: Arc<dyn SecretsProvider>,
    llm: DynLlm,
    resolved_config: Option<Arc<ReactResolvedConfig>>,
    trace_tx: Option<UnboundedSender<String>>,
    vector: Option<Arc<dyn VectorStore>>,
    state: Option<Arc<dyn StateStore>>,
    capabilities: CapabilityMap,
}

impl SuiteCtxBuilder {
    pub fn new(
        storage: Arc<dyn StorageAdapter>,
        secrets: Arc<dyn SecretsProvider>,
        llm: DynLlm,
        scope: RequestScope,
        keyspace: Arc<dyn Keyspace>,
    ) -> Self {
        Self {
            storage,
            scope,
            keyspace,
            secrets,
            llm,
            resolved_config: None,
            trace_tx: None,
            vector: None,
            state: None,
            capabilities: CapabilityMap::default(),
        }
    }

    pub fn resolved_config(mut self, v: Option<Arc<ReactResolvedConfig>>) -> Self { self.resolved_config = v; self }
    pub fn trace_tx(mut self, v: Option<UnboundedSender<String>>) -> Self { self.trace_tx = v; self }
    pub fn vector(mut self, v: Option<Arc<dyn VectorStore>>) -> Self { self.vector = v; self }
    pub fn state(mut self, v: Option<Arc<dyn StateStore>>) -> Self { self.state = v; self }
    pub fn capabilities(mut self, v: CapabilityMap) -> Self { self.capabilities = v; self }

    pub fn build(self) -> SuiteCtx {
        SuiteCtx {
            storage: self.storage,
            scope: self.scope,
            keyspace: self.keyspace,
            secrets: self.secrets,
            llm: self.llm,
            resolved_config: self.resolved_config,
            trace_tx: self.trace_tx,
            vector: self.vector,
            state: self.state,
            capabilities: self.capabilities,
        }
    }
}

/// No-op storage used only for the `SuiteCtx` default (test convenience).
/// Real adapters live in `modules/adaptors/storage-*` crates.
#[derive(Default)]
struct NullStorageAdapter;

#[async_trait]
impl StorageAdapter for NullStorageAdapter {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        Err(CoreError::Storage(format!("NullStorageAdapter: get_json('{}')", key)))
    }
    async fn put_json(&self, key: &str, _value: &Value) -> Result<(), CoreError> {
        Err(CoreError::Storage(format!("NullStorageAdapter: put_json('{}')", key)))
    }
    async fn put_json_if_etag_matches(
        &self,
        key: &str,
        _value: &Value,
        _expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError> {
        Err(CoreError::Storage(format!(
            "NullStorageAdapter: put_json_if_etag_matches('{}')",
            key
        )))
    }
    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::Storage(format!("NullStorageAdapter: get_bytes('{}')", key)))
    }
    async fn put_bytes(&self, key: &str, _bytes: &[u8], _ct: &str) -> Result<(), CoreError> {
        Err(CoreError::Storage(format!("NullStorageAdapter: put_bytes('{}')", key)))
    }
    async fn delete_object(&self, key: &str) -> Result<(), CoreError> {
        Err(CoreError::Storage(format!("NullStorageAdapter: delete_object('{}')", key)))
    }
    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        Err(CoreError::Storage(format!("NullStorageAdapter: head_etag('{}')", key)))
    }
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError> {
        Err(CoreError::Storage(format!("NullStorageAdapter: list_prefix('{}')", prefix)))
    }
}

impl Default for SuiteCtx {
    fn default() -> Self {
        Self {
            storage: Arc::new(NullStorageAdapter),
            scope: RequestScope::parse("default", "default", "default")
                .expect("default scope segments are safe"),
            keyspace: Arc::new(DefaultKeyspace::new("unset".to_string())),
            secrets: Arc::new(NullSecretsProvider::default()),
            llm: Arc::new(NullModel::new()),
            resolved_config: None,
            trace_tx: None,
            vector: None,
            state: None,
            capabilities: CapabilityMap::default(),
        }
    }
}

#[async_trait]
pub trait Suite: Send + Sync {
    fn id(&self) -> &'static str;

    fn label(&self) -> &'static str {
        self.id()
    }

    fn supported_agent_types(&self) -> Vec<String> {
        vec!["ask".to_string()]
    }

    fn default_agent_type(&self) -> &'static str {
        "ask"
    }

    fn phase_order(&self, _agent_type: &str) -> Vec<String> {
        Vec::new()
    }

    fn initial_phase(&self) -> &'static str {
        "preflight"
    }

    async fn load_ws_plans(
        &self,
        _thread_id: &str,
        _ctx: &SuiteCtx,
    ) -> Result<Vec<serde_json::Value>, String> {
        Ok(Vec::new())
    }

    async fn handle_new(
        &self,
        thread_id: &str,
        question: &str,
        agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String>;

    async fn handle_open(
        &self,
        thread_id: &str,
        question: &str,
        agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String>;

    async fn handle_user(
        &self,
        thread_id: &str,
        text: &str,
        agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String>;
}

/// Compile-time workflow contract for suites using the core workflow kernel.
pub trait WorkflowSuiteContract {
    type Phase: Copy + Eq + Send + Sync + 'static;
    type ReasonCode: Copy + Eq + Send + Sync + 'static;
    type GuardKind: Copy + Eq + Send + Sync + 'static;
    type State: Clone + Send + Sync + 'static;
    type Event: Send + Sync + 'static;

    fn phase_as_str(phase: Self::Phase) -> &'static str;
    fn reason_as_str(reason: Self::ReasonCode) -> &'static str;
    fn guard_kind_as_str(kind: Self::GuardKind) -> &'static str;
    fn is_backtrack(from: Self::Phase, to: Self::Phase) -> bool;
    fn replan_backtrack_cap() -> usize;
    fn pre_turn(_state: &Self::State) -> crate::workflow::PreTurnDirective<Self::GuardKind> {
        crate::workflow::PreTurnDirective::Proceed
    }

    fn reduce(state: &mut Self::State, event: Self::Event);
}

/// Typed node contract for suites that use an explicit workflow state-machine node.
pub trait WorkflowNodeContract: WorkflowSuiteContract {
    type Node: Copy + Eq + Send + Sync + 'static;

    fn node_from_state(state: &Self::State) -> Self::Node;
    fn phase_from_node(node: Self::Node) -> Self::Phase;
}

pub type DynSuite = Arc<dyn Suite>;

pub struct SuiteRegistry {
    suites: HashMap<&'static str, DynSuite>,
}

impl SuiteRegistry {
    pub fn new() -> Self {
        Self {
            suites: HashMap::new(),
        }
    }

    pub fn register<S: Suite + 'static>(&mut self, suite: S) {
        let id = suite.id();
        self.suites.insert(id, Arc::new(suite));
    }

    pub fn get(&self, suite_id: &str) -> Option<DynSuite> {
        self.suites.get(suite_id).cloned()
    }

    pub fn list_ids(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = self.suites.keys().copied().collect();
        out.sort();
        out
    }
}
