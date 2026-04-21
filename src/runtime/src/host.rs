use async_trait::async_trait;
use react_core::resolved_config::ReactResolvedConfig;
use react_core::suite::{SuiteCtx, SuiteRegistry};

/// App-owned composition seam for wiring suites into the generic runtime.
#[async_trait]
pub trait HostComposition: Send + Sync {
    /// Register the suites this host wants to expose.
    fn register_suites(&self, registry: &mut SuiteRegistry);

    /// Resolve host-owned suite config from the raw config file payload.
    fn resolve_suite_config(
        &self,
        raw_suite_config: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        Ok(raw_suite_config)
    }

    /// Extend the base suite context with host-specific capabilities/providers.
    async fn configure_suite_ctx(
        &self,
        _cfg: &ReactResolvedConfig,
        _suite_ctx: &mut SuiteCtx,
    ) -> Result<(), String> {
        Ok(())
    }
}

pub fn registry_from_host(host: &dyn HostComposition) -> SuiteRegistry {
    let mut registry = SuiteRegistry::new();
    host.register_suites(&mut registry);
    registry
}
