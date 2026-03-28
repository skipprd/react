use async_trait::async_trait;

/// Decision returned by an [`InterruptPolicy`] for `await_user` / `await_approval` events.
#[derive(Debug, Clone)]
pub enum InterruptDecision {
    /// Forward the prompt to the user (WS/terminal) and wait for a reply.
    Forward,
    /// Automatically approve (e.g. headless CI with auto-approve).
    AutoApprove,
    /// Reject the interrupt with an error (e.g. headless without auto-approve).
    Reject { reason: String },
}

/// Decision returned by an [`InterruptPolicy`] for review prompts.
#[derive(Debug, Clone)]
pub enum ReviewDecision {
    Forward,
    AutoApprove,
    Reject { reason: String },
}

/// Policy-based handler for agent interrupts (await_user, await_approval, review).
///
/// Injected into the suite runner so that control-flow decisions about how to
/// handle interrupts are decoupled from the presentation layer.
#[async_trait]
pub trait InterruptPolicy: Send + Sync {
    async fn on_await_user(&self, prompt: &str) -> InterruptDecision;
    async fn on_await_approval(&self, prompt: &str) -> InterruptDecision;
    async fn on_review(&self, text: &str, meta: Option<&serde_json::Value>) -> ReviewDecision;
}

/// WS-connected policy: forwards all interrupts to the user.
pub struct WsInterruptPolicy;

#[async_trait]
impl InterruptPolicy for WsInterruptPolicy {
    async fn on_await_user(&self, _prompt: &str) -> InterruptDecision {
        InterruptDecision::Forward
    }
    async fn on_await_approval(&self, _prompt: &str) -> InterruptDecision {
        InterruptDecision::Forward
    }
    async fn on_review(&self, _text: &str, _meta: Option<&serde_json::Value>) -> ReviewDecision {
        ReviewDecision::Forward
    }
}

/// Headless auto-approve policy: automatically approves all interrupts.
pub struct AutoApprovePolicy;

#[async_trait]
impl InterruptPolicy for AutoApprovePolicy {
    async fn on_await_user(&self, _prompt: &str) -> InterruptDecision {
        InterruptDecision::Reject {
            reason: "ask_user_not_supported_in_headless".to_string(),
        }
    }
    async fn on_await_approval(&self, _prompt: &str) -> InterruptDecision {
        InterruptDecision::AutoApprove
    }
    async fn on_review(&self, _text: &str, _meta: Option<&serde_json::Value>) -> ReviewDecision {
        ReviewDecision::AutoApprove
    }
}

/// Headless reject policy: rejects all interrupts that require user input.
pub struct RejectPolicy;

#[async_trait]
impl InterruptPolicy for RejectPolicy {
    async fn on_await_user(&self, prompt: &str) -> InterruptDecision {
        InterruptDecision::Reject {
            reason: format!("ask_user_not_supported_in_headless: {prompt}"),
        }
    }
    async fn on_await_approval(&self, prompt: &str) -> InterruptDecision {
        InterruptDecision::Reject {
            reason: format!("await_approval_not_supported_in_headless: {prompt}"),
        }
    }
    async fn on_review(&self, _text: &str, _meta: Option<&serde_json::Value>) -> ReviewDecision {
        ReviewDecision::Reject {
            reason: "review_not_supported_in_headless".to_string(),
        }
    }
}
