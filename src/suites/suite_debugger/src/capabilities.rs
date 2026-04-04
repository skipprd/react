use react_core::agent::AgentCtx;
use react_core::llm::ReasoningEffort;
use react_core::scope::RequestScope;
use react_core::suite::SuiteCtx;

#[derive(Clone, Debug)]
pub struct DebugTargetScope {
    pub scope: RequestScope,
}

#[derive(Clone, Debug)]
pub struct SuiteDebuggerConfig {
    pub reasoning_effort: ReasoningEffort,
    pub strict_audit: bool,
    pub enable_admin_repo_query: bool,
}

impl Default for SuiteDebuggerConfig {
    fn default() -> Self {
        Self {
            reasoning_effort: ReasoningEffort::Medium,
            strict_audit: false,
            enable_admin_repo_query: false,
        }
    }
}

pub fn target_scope_for_suite(ctx: &SuiteCtx) -> RequestScope {
    ctx.capability::<DebugTargetScope>()
        .map(|cap| cap.scope.clone())
        .unwrap_or_else(|| ctx.scope().clone())
}

pub fn target_scope_for_agent(ctx: &AgentCtx) -> RequestScope {
    ctx.capability::<DebugTargetScope>()
        .map(|cap| cap.scope.clone())
        .unwrap_or_else(|| ctx.scope().clone())
}

pub fn debugger_config(ctx: &SuiteCtx) -> SuiteDebuggerConfig {
    ctx.capability::<SuiteDebuggerConfig>()
        .map(|cfg| (*cfg).clone())
        .unwrap_or_default()
}
