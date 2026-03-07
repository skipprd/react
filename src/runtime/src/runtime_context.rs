use std::sync::Arc;
use react_core::resolved_config::ReactResolvedConfig;

/// Shared runtime context, threaded through the application rather than stored in globals.
/// Currently wraps the resolved config. Future: LLM router, terminal sink, etc.
#[derive(Clone)]
pub struct RuntimeContext {
    pub config: Arc<ReactResolvedConfig>,
}
