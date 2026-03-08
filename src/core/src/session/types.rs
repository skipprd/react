use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;

pub const THREAD_SCHEMA_VERSION: u32 = 4;
pub const THREAD_STATE_SCHEMA_VERSION: u32 = 2;
pub const CONTROL_STATE_ENVELOPE_SCHEMA_VERSION: u32 = 1;

/// Generic thread-level control state envelope.
///
/// Core owns this wrapper to provide a stable mutation contract while keeping
/// suite payloads opaque (`payload`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ControlStateEnvelope {
    pub schema_version: u32,
    pub suite_id: String,
    pub payload: Value,
}

/// Materialized, reloadable thread state (stable summary, not raw streaming events).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThreadState {
    pub thread_state_schema_version: u32,
    pub thread_id: String,
    #[serde(default)]
    pub suite_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub current_phase: Option<String>,
    /// Number of thread steps that have been materialized into this state snapshot.
    #[serde(default)]
    pub last_materialized_step_count: usize,
    /// Sum of completed phase runtimes in milliseconds.
    #[serde(default)]
    pub total_runtime_ms: u64,
    /// Per-item semaphore/state keyed by stable item ids.
    #[serde(default)]
    pub items: BTreeMap<String, ThreadItemState>,
    /// Opaque suite-owned state snapshot.
    #[serde(default)]
    pub suite_state: Option<Value>,
    /// Opaque suite-owned control snapshot (authoritative control state).
    #[serde(default)]
    pub control_state: Option<Value>,
    /// Thread-scoped bootstrap statuses (catalog/discovery readiness, etc).
    #[serde(default)]
    pub bootstrap: ThreadBootstrapState,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ThreadBootstrapState {
    /// Suite-opaque extension data. Each suite serializes its own bootstrap state here.
    #[serde(default)]
    pub extensions: Value,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThreadEvent {
    pub step_idx: usize,
    pub event_kind: ThreadEventKind,
    pub ts: String,
    #[serde(default)]
    pub tool_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub clean_name: Option<String>,
    #[serde(default)]
    pub status: Option<ThreadEventStatus>,
    #[serde(default)]
    pub runtime_ms: Option<u64>,
    #[serde(default)]
    pub payload: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub call_id: Option<u64>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub phase: Option<String>,

    pub ctx: Option<ExecutionContext>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ThreadEventKind {
    #[default]
    ToolStart,
    ToolEnd,
    LlmStart,
    LlmEnd,
}

impl ThreadEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ThreadEventKind::ToolStart => "tool_start",
            ThreadEventKind::ToolEnd => "tool_end",
            ThreadEventKind::LlmStart => "llm_start",
            ThreadEventKind::LlmEnd => "llm_end",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ThreadEventStatus {
    Running,
    Ok,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolStepStatus {
    Running,
    Ok,
    Failed,
}

impl ToolStepStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolStepStatus::Running => "running",
            ToolStepStatus::Ok => "ok",
            ToolStepStatus::Failed => "failed",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LlmStepStatus {
    Ok,
    Failed,
}

impl From<ToolStepStatus> for ThreadEventStatus {
    fn from(s: ToolStepStatus) -> Self {
        match s {
            ToolStepStatus::Running => ThreadEventStatus::Running,
            ToolStepStatus::Ok => ThreadEventStatus::Ok,
            ToolStepStatus::Failed => ThreadEventStatus::Failed,
        }
    }
}

impl From<LlmStepStatus> for ThreadEventStatus {
    fn from(s: LlmStepStatus) -> Self {
        match s {
            LlmStepStatus::Ok => ThreadEventStatus::Ok,
            LlmStepStatus::Failed => ThreadEventStatus::Failed,
        }
    }
}

/// Artifact kind as a plain string. Suites define their own constants (e.g. "model", "metric").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ArtifactKind(pub String);

impl ArtifactKind {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ArtifactKind {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<ArtifactKind> for String {
    fn from(value: ArtifactKind) -> Self {
        value.0
    }
}

impl Serialize for ArtifactKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ArtifactKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(ArtifactKind(s))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArtifactSaveStatus {
    Added,
    Modified,
    Ok,
    Failed,
    Other(String),
}

impl ArtifactSaveStatus {
    pub fn as_str(&self) -> &str {
        match self {
            ArtifactSaveStatus::Added => "added",
            ArtifactSaveStatus::Modified => "modified",
            ArtifactSaveStatus::Ok => "ok",
            ArtifactSaveStatus::Failed => "failed",
            ArtifactSaveStatus::Other(s) => s.as_str(),
        }
    }
}

impl From<String> for ArtifactSaveStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "added" => ArtifactSaveStatus::Added,
            "modified" => ArtifactSaveStatus::Modified,
            "ok" => ArtifactSaveStatus::Ok,
            "failed" => ArtifactSaveStatus::Failed,
            _ => ArtifactSaveStatus::Other(value),
        }
    }
}

impl From<ArtifactSaveStatus> for String {
    fn from(value: ArtifactSaveStatus) -> Self {
        match value {
            ArtifactSaveStatus::Added => "added".to_string(),
            ArtifactSaveStatus::Modified => "modified".to_string(),
            ArtifactSaveStatus::Ok => "ok".to_string(),
            ArtifactSaveStatus::Failed => "failed".to_string(),
            ArtifactSaveStatus::Other(s) => s,
        }
    }
}

impl Serialize for ArtifactSaveStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ArtifactSaveStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(ArtifactSaveStatus::from(s))
    }
}

/// Opaque execution context for hierarchical UI rendering.
///
/// Suites own the schema; core only persists/forwards this payload.
/// All fields are stored in a generic `data` map using string keys.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ExecutionContext {
    #[serde(flatten)]
    pub data: BTreeMap<String, Value>,
}

impl ExecutionContext {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    pub fn set(&mut self, key: impl Into<String>, val: Value) {
        self.data.insert(key.into(), val);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct ExecutionPlanKind(pub String);

impl ExecutionPlanKind {
    pub fn new(kind: impl Into<String>) -> Self {
        Self(kind.into())
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ThreadItemKind {
    #[default]
    Phase,
    Tool,
    Task,
}

impl ThreadItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ThreadItemKind::Phase => "phase",
            ThreadItemKind::Tool => "tool",
            ThreadItemKind::Task => "task",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ThreadItemStatus {
    Queued,
    #[default]
    Running,
    Ok,
    Failed,
    Blocked,
}

impl ThreadItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ThreadItemStatus::Queued => "queued",
            ThreadItemStatus::Running => "running",
            ThreadItemStatus::Ok => "ok",
            ThreadItemStatus::Failed => "failed",
            ThreadItemStatus::Blocked => "blocked",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThreadItemState {
    pub kind: ThreadItemKind,
    pub status: ThreadItemStatus,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
    /// Runtime/duration in milliseconds (best-effort).
    #[serde(default)]
    pub runtime_ms: Option<u64>,
    #[serde(default)]
    pub last_error: Option<ThreadItemError>,
    #[serde(default)]
    pub outputs: Option<Value>,
}

impl ThreadItemState {
    pub fn phase_running(ts: &str) -> Self {
        Self {
            kind: ThreadItemKind::Phase,
            status: ThreadItemStatus::Running,
            started_at: Some(ts.to_string()),
            finished_at: None,
            runtime_ms: None,
            last_error: None,
            outputs: None,
        }
    }

    pub fn phase_finished(ts: &str) -> Self {
        Self {
            kind: ThreadItemKind::Phase,
            status: ThreadItemStatus::Ok,
            started_at: None,
            finished_at: Some(ts.to_string()),
            runtime_ms: None,
            last_error: None,
            outputs: None,
        }
    }

    pub fn phase_blocked(ts: &str) -> Self {
        Self {
            kind: ThreadItemKind::Phase,
            status: ThreadItemStatus::Blocked,
            started_at: Some(ts.to_string()),
            finished_at: None,
            runtime_ms: None,
            last_error: None,
            outputs: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThreadItemError {
    pub summary: String,
    #[serde(default)]
    pub tool_step_idx: Option<usize>,
    #[serde(default)]
    pub step_ts: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub ok: bool,
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl Observation {
    pub fn ok() -> Self {
        Self {
            ok: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn fail(errors: Vec<String>) -> Self {
        Self {
            ok: false,
            errors,
            warnings: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolObservation {
    pub ok: bool,
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Tool-specific payload (written_keys, rows, etc.).
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ToolObservation {
    pub fn ok(extra: BTreeMap<String, Value>) -> Self {
        Self {
            ok: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            extra,
        }
    }

    pub fn fail(errors: Vec<String>, extra: BTreeMap<String, Value>) -> Self {
        Self {
            ok: false,
            errors,
            warnings: Vec::new(),
            extra,
        }
    }

    fn non_empty_string_field(extra: &BTreeMap<String, Value>, key: &str) -> Option<String> {
        extra
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn extract_errors_from_extra(extra: &BTreeMap<String, Value>) -> Vec<String> {
        if let Some(Value::Array(arr)) = extra.get("errors") {
            let out: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect();
            if !out.is_empty() {
                return out;
            }
        }
        if let Some(v) = extra.get("error") {
            match v {
                Value::String(s) => {
                    let t = s.trim();
                    if !t.is_empty() {
                        return vec![t.to_string()];
                    }
                }
                other => {
                    let s = other.to_string();
                    if !s.trim().is_empty() {
                        return vec![s];
                    }
                }
            }
        }
        for key in ["message", "reason", "detail"] {
            if let Some(s) = Self::non_empty_string_field(extra, key) {
                return vec![s];
            }
        }
        Vec::new()
    }

    pub fn first_error_or_context(&self) -> Option<String> {
        self.errors.first().cloned().or_else(|| {
            for key in ["message", "reason", "detail"] {
                if let Some(s) = Self::non_empty_string_field(&self.extra, key) {
                    return Some(s);
                }
            }
            None
        })
    }

    /// Normalize a tool output `Value` into the canonical envelope:
    /// - `errors` is ALWAYS present (even if 0/1)
    /// - singular `error: string` is promoted to `errors: [error]` and removed from `extra`
    pub fn normalize(v: Value) -> Self {
        let mut extra: BTreeMap<String, Value> = match v {
            Value::Object(m) => m.into_iter().collect(),
            other => {
                let mut m = BTreeMap::new();
                m.insert("raw".to_string(), other);
                m
            }
        };

        let ok = extra.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);

        let errors = Self::extract_errors_from_extra(&extra);

        let warnings = if let Some(Value::Array(arr)) = extra.get("warnings") {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let mut errors = errors;

        extra.remove("ok");
        extra.remove("errors");
        extra.remove("warnings");
        extra.remove("error");

        if !ok && errors.is_empty() {
            errors.push("no error details were captured".to_string());
        }

        Self {
            ok,
            errors,
            warnings,
            extra,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ThreadStep {
    SwitchSuite {
        from: Option<String>,
        to: String,
        observation: Observation,
        ts: String,
        agent: String,
    },
    SwitchAgent {
        from: Option<String>,
        to: String,
        observation: Observation,
        ts: String,
        agent: String,
    },
    User {
        text: String,
        observation: Observation,
        ts: String,
        agent: String,
    },
    ToolStart {
        tool_id: String,
        name: String,
        /// Human-readable, short label for UI (e.g. "Read config.yml").
        #[serde(default)]
        clean_name: String,
        args: Value,
        /// running|ok|failed (tool_start should be running)
        status: ToolStepStatus,
        #[serde(default)]
        payload: Option<Value>,
        ctx: Option<ExecutionContext>,
        ts: String,
        agent: String,
    },
    ToolEnd {
        tool_id: String,
        name: String,
        /// Human-readable, short label for UI (e.g. "Read config.yml").
        #[serde(default)]
        clean_name: String,
        #[serde(default)]
        args: Value,
        /// running|ok|failed (tool_end should be ok|failed)
        status: ToolStepStatus,
        #[serde(default)]
        payload: Option<Value>,
        ctx: Option<ExecutionContext>,
        observation: ToolObservation,
        ts: String,
        agent: String,
    },
    LlmStart {
        call_id: u64,
        #[serde(default)]
        model: Option<String>,
        phase: String,
        ctx: Option<ExecutionContext>,
        ts: String,
        agent: String,
    },
    LlmEnd {
        call_id: u64,
        #[serde(default)]
        model: Option<String>,
        phase: String,
        status: LlmStepStatus, // ok|failed
        #[serde(default)]
        error: Option<String>,
        ctx: Option<ExecutionContext>,
        ts: String,
        agent: String,
    },
    /// LLM call observability (hashed/deduped prompt parts + response).
    ///
    /// Notes:
    /// - `parts` MUST constitute the entire prompt (including system prompts, hardcoded strings, etc.).
    /// - Each element of `parts` is a JSON object with (at minimum) `{name, hash, text}` where
    ///   `text` is either the full (redacted) part content or the literal string `"unchanged"`.
    /// - `prompt_hash` is sha256 over the exact serialized message list used in the call.
    LlmCall {
        call_id: u64,
        /// Best-effort model identifier (provider/model name).
        model: String,
        /// Best-effort phase identifier (suite phase or core react loop label).
        phase: String,
        /// sha256 hex of the exact serialized message list (system/user/tool).
        prompt_hash: String,
        /// Prompt parts in order; each part may include `"text":"unchanged"` when deduped.
        parts: Vec<Value>,
        /// Stable part hashes keyed by part name.
        part_hashes: BTreeMap<String, String>,
        /// sha256 hex of the raw response text.
        response_hash: String,
        /// Full (redacted) response text when enabled.
        #[serde(default)]
        response_text: Option<String>,
        observation: Observation,
        ts: String,
        agent: String,
    },
    Phase {
        phase: String,
        from_phase: Option<String>,
        #[serde(default)]
        reason_code: Option<String>,
        reason_detail: Option<Value>,
        observation: Observation,
        ts: String,
        agent: String,
    },
    GuardBlock {
        phase: String,
        kind: String,
        reason: String,
        observation: Observation,
        ts: String,
        agent: String,
    },
    ArtifactFocus {
        kind: ArtifactKind,
        name: String,
        entity_id: Option<String>,
        exists: bool,
        observation: Observation,
        ts: String,
        agent: String,
    },
    ArtifactSaved {
        kind: ArtifactKind,
        name: String,
        entity_id: Option<String>,
        key: String,
        status: ArtifactSaveStatus,
        lines_added: u64,
        lines_removed: u64,
        observation: Observation,
        ts: String,
        agent: String,
    },
    Interrupt {
        kind: String,
        prompt: String,
        observation: Observation,
        ts: String,
        agent: String,
    },
    ReviewResponse {
        text: String,
        #[serde(default)]
        meta: Option<Value>,
        observation: Observation,
        ts: String,
        agent: String,
    },
    Complete {
        kind: String,
        payload: Value,
        #[serde(default)]
        display: Option<String>,
        observation: Observation,
        ts: String,
        agent: String,
    },
    Checkpoint {
        kind: String,
        payload: Value,
        #[serde(default)]
        display: Option<String>,
        observation: Observation,
        ts: String,
        agent: String,
    },
}

macro_rules! thread_step_ts {
    ($($variant:ident),* $(,)?) => {
        impl ThreadStep {
            pub fn ts(&self) -> &str {
                match self { $(Self::$variant { ts, .. } => ts,)* }
            }
        }
    };
}

thread_step_ts!(
    SwitchSuite, SwitchAgent, User, ToolStart, ToolEnd,
    LlmStart, LlmEnd, LlmCall, Phase, GuardBlock,
    ArtifactFocus, ArtifactSaved, Interrupt, ReviewResponse,
    Complete, Checkpoint,
);

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ThreadLog {
    pub schema_version: u32,
    pub steps: Vec<ThreadStep>,
    pub result: Option<ThreadResult>,
    pub title: Option<String>,
    pub title_locked: bool,
}

impl Default for ThreadLog {
    fn default() -> Self {
        Self {
            schema_version: THREAD_SCHEMA_VERSION,
            steps: Vec::new(),
            result: None,
            title: None,
            title_locked: false,
        }
    }
}

impl ThreadLog {
    pub fn current_phase(&self) -> Option<&str> {
        for step in self.steps.iter().rev() {
            if let ThreadStep::Phase { phase, .. } = step {
                let t = phase.trim();
                if !t.is_empty() {
                    return Some(t);
                }
            }
        }
        None
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ThreadResult {
    pub kind: String,
    pub payload: Value,
    #[serde(default)]
    pub display: Option<String>,
}
