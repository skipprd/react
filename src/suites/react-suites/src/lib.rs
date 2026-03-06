//! Suites for the ReAct runtime.
//!
//! Each suite is a self-contained product workflow that depends only on `react-core`.

pub mod data_engineer;
pub mod kb;

pub use react_core::resolved_config::ReactResolvedConfig;
pub use react_core::suite::{DynSuite, FlowFrame, Suite, SuiteCtx, SuiteRegistry};

pub fn default_registry() -> SuiteRegistry {
    let mut reg = SuiteRegistry::new();
    reg.register(data_engineer::DataEngineerSuite);
    reg.register(kb::KbSuite);
    reg
}
