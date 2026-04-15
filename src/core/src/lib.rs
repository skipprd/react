//! ReAct runner core (usecase-agnostic).
//!
//! This crate intentionally contains only:
//! - The ReAct loop (`agent`)
//! - Tool interface/registry (`tools`)
//! - Thread transcript persistence (`session`)
//! - Minimal capability traits/types (LLM + storage + scope/keyspace + optional providers)

pub mod agent;
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
pub mod provider_traits;
pub mod resolved_config;
pub mod schema_registry;
pub mod scope;
pub mod session;
pub mod storage;
pub mod suite;
pub mod thread_ctx;
pub mod thread_feedback;
pub mod tools;
pub mod workflow;

#[cfg(test)]
pub(crate) mod test_support;

pub use error::{CoreError, CoreResult};

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
