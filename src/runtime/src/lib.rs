//! ReAct agent runtime (library).
//!
//! Runtime crate (CLI + WS server + concrete provider implementations).

pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod llm;
pub mod run_engine;
pub mod runtime_context;
pub mod runtime_settings;
pub mod thread_logs;
pub(crate) mod wiring;
