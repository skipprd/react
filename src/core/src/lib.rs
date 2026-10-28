//! ReAct runner core (usecase-agnostic).
//!
//! This crate intentionally contains only:
//! - The ReAct loop (`agent`)
//! - Tool interface/registry (`tools`)
//! - Thread transcript persistence (`session`)
//! - Minimal capability traits/types (LLM + storage + scope/keyspace + optional providers)

pub mod agent;
pub mod budget;
pub mod capability;
pub mod discover;
pub mod error;
pub mod error_context;
pub mod helpers;
pub mod interrupt;
pub mod json_repair;
pub mod keyspace;
pub mod llm;
pub mod llm_observability;
pub mod parse_error;
pub mod provider_traits;
pub mod resolved_config;
pub mod retry;
pub mod schema_registry;
pub mod scope;
pub mod session;
pub mod storage;
pub mod suite;
pub mod text;
pub mod thread_ctx;
pub mod thread_feedback;
pub mod tools;
pub mod workflow;

#[cfg(test)]
pub(crate) mod test_support;

pub use agent::{Entry, ErrorLedger, PromptBudget, Transcript};
pub use budget::{
    BudgetExhausted, BudgetMeter, Cancel, Charge, RunBudget, MAX_AUTO_RESUMES,
    MAX_REPEATED_ACTION_FAILURES,
};
pub use error::{CoreError, CoreResult};
pub use llm::{openai_api_for_model, LlmError, OpenAiApi};
pub use parse_error::StepParseError;
pub use retry::{retry_classified, Classify};
pub use storage::StorageError;
pub use tools::{bound_tool_value, DuplicateTool, ToolError, ToolOutcome, MAX_OUTPUT_BYTES};

/// Check whether an environment variable is set to a truthy value (`1`, `true`, or `yes`).
pub fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| {
            let vv = v.trim().to_lowercase();
            vv == "1" || vv == "true" || vv == "yes"
        })
        .unwrap_or(false)
}
