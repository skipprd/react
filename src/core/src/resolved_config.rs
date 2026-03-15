use crate::scope::RequestScope;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// ── Enums that replace stringly-typed dispatch ──────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageMode {
    Local,
    S3,
}

impl fmt::Display for StorageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::S3 => write!(f, "s3"),
        }
    }
}

impl Default for StorageMode {
    fn default() -> Self {
        Self::Local
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LlmProvider {
    Null,
    OpenaiCompat,
    Openai,
    Http,
    LlamaCpp,
}

impl fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::OpenaiCompat => write!(f, "OPENAI_COMPAT"),
            Self::Openai => write!(f, "OPENAI"),
            Self::Http => write!(f, "HTTP"),
            Self::LlamaCpp => write!(f, "LLAMA_CPP"),
        }
    }
}

impl Default for LlmProvider {
    fn default() -> Self {
        Self::Null
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigParseError {
    pub value: String,
}

impl fmt::Display for ConfigParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unsupported llm provider '{}' (expected one of: NULL, OPENAI_COMPAT, OPENAI, HTTP, LLAMA_CPP)",
            self.value
        )
    }
}

impl std::error::Error for ConfigParseError {}

impl FromStr for LlmProvider {
    type Err = ConfigParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.trim();
        match raw.to_ascii_uppercase().as_str() {
            "OPENAI_COMPAT" => Ok(Self::OpenaiCompat),
            "OPENAI" => Ok(Self::Openai),
            "HTTP" => Ok(Self::Http),
            "LLAMA_CPP" => Ok(Self::LlamaCpp),
            "NULL" => Ok(Self::Null),
            _ => Err(ConfigParseError {
                value: raw.to_string(),
            }),
        }
    }
}

// ── Resolved config structs ─────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ReactResolvedConfig {
    pub server: ServerResolved,
    pub storage: StorageResolved,
    pub scope: RequestScope,
    pub llm: LlmResolved,
    /// Suite-specific configuration, opaque to core.
    /// Each suite deserializes its own config from this value.
    pub suite_config: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct ServerResolved {
    pub port: u16,
}

#[derive(Clone, Debug)]
pub struct StorageResolved {
    pub mode: StorageMode,
    pub bucket: Option<String>,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct LlmResolved {
    pub provider: LlmProvider,
    pub base_url: Option<String>,
    pub reason_model: Option<String>,
    pub task_model: Option<String>,
    pub embed_model: Option<String>,
    pub context_length: Option<usize>,
    pub gpu_layers: Option<usize>,
    pub http_timeout_secs: Option<u64>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
}
