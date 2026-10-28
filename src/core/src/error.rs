use thiserror::Error;

use crate::llm::LlmError;
use crate::parse_error::StepParseError;
use crate::retry::Classify;
use crate::storage::StorageError;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("{0}")]
    Generic(String),

    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("{0}")]
    Llm(#[from] LlmError),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("session error: {0}")]
    Session(String),

    #[error("schema error: {0}")]
    Schema(String),

    #[error("agent error: {0}")]
    Agent(String),

    #[error("parse error: {0}")]
    Parse(#[from] StepParseError),

    #[error("audit write conflict for '{0}'")]
    AuditConflict(String),

    #[error("audit unavailable for '{thread}' after {retries} conflicts")]
    AuditUnavailable { thread: String, retries: u8 },

    #[error("keyspace error: {0}")]
    Keyspace(String),

    #[error("budget: {0}")]
    Budget(#[from] crate::budget::BudgetExhausted),
}

impl CoreError {
    pub fn generic(msg: impl Into<String>) -> Self {
        CoreError::Generic(msg.into())
    }
}

impl Classify for CoreError {
    fn retry(&self) -> bool {
        match self {
            CoreError::Llm(err) => err.retry(),
            CoreError::Storage(err) => err.retry(),
            _ => false,
        }
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
