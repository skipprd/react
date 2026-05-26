use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteAction {
    #[default]
    New,
    Open,
    User,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecuteScopeOverride {
    #[serde(default)]
    pub tenant: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ExecuteContext {
    #[serde(default)]
    pub scope: Option<ExecuteScopeOverride>,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExecuteRequest {
    pub suite_id: String,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub thread_id: Option<String>,
    pub question: String,
    #[serde(default)]
    pub action: ExecuteAction,
    #[serde(default)]
    pub context: Option<ExecuteContext>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExecuteResponse {
    pub thread_id: String,
    pub agent_type: String,
    pub frames: Vec<FlowFrame>,
}

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum FlowFrame {
    Complete {
        kind: FlowKind,
        payload: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<String>,
    },
    Review {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<Value>,
    },
    Checkpoint {
        kind: FlowKind,
        payload: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<String>,
    },
    Interrupt {
        kind: FlowKind,
        prompt: String,
    },
}

pub fn terminal_frame(frames: &[FlowFrame]) -> Option<&FlowFrame> {
    frames.iter().rev().find(|frame| match frame {
        FlowFrame::Complete { .. } | FlowFrame::Review { .. } | FlowFrame::Interrupt { .. } => true,
        FlowFrame::Checkpoint { .. } => false,
    })
}
