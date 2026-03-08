use std::sync::Arc;
use react_core::resolved_config::ReactResolvedConfig;

/// Shared runtime context, threaded through the application rather than stored in globals.
/// Currently wraps the resolved config.
///
/// TODO: progressively absorb the remaining process-global singletons:
/// - `LlmRouter` (currently constructed ad-hoc)
/// - `TerminalSink` (ws::terminal::SINK)
/// - scope / keyspace (runtime_settings::SCOPE_PREFERENCE)
/// Once those live here, the `RESOLVED_CONFIG` singleton can be removed.
#[derive(Clone)]
pub struct RuntimeContext {
    pub config: Arc<ReactResolvedConfig>,
}
