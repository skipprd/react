//! ReAct runtime host library.
//!
//! This crate exposes suite-neutral config/bootstrap/execution frontends. Apps
//! such as `skippr` and `goggles-reactd` own suite registration and
//! host-specific wiring via `host::HostComposition`.

pub mod bootstrap;
pub mod config;
pub mod execute;
pub mod host;
pub mod http;
pub mod llm;
pub mod run_engine;
pub mod runtime_context;
pub mod runtime_settings;
pub(crate) mod secrets;
pub mod thread_logs;
