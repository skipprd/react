use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("{0}")]
    Generic(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("session error: {0}")]
    Session(String),

    #[error("schema error: {0}")]
    Schema(String),

    #[error("agent error: {0}")]
    Agent(String),

    #[error("keyspace error: {0}")]
    Keyspace(String),
}

pub type CoreResult<T> = Result<T, CoreError>;

impl From<String> for CoreError {
    fn from(s: String) -> Self {
        CoreError::Generic(s)
    }
}

impl From<&str> for CoreError {
    fn from(s: &str) -> Self {
        CoreError::Generic(s.to_string())
    }
}
