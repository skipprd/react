use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const THREAD_STATE_SCHEMA_VERSION: u32 = 2;

/// Disposable, materialized projection of the ThreadLog for display purposes.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ThreadLogViewCache {
    pub thread_state_schema_version: u32,
    pub thread_id: String,
    #[serde(default)]
    pub suite_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub current_phase: Option<String>,
    #[serde(default)]
    pub last_materialized_step_count: usize,
    #[serde(default)]
    pub total_runtime_ms: u64,
    #[serde(default)]
    pub items: BTreeMap<String, ThreadItemState>,
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
