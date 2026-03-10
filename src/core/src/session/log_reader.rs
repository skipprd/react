use async_trait::async_trait;

use super::ThreadLog;
use crate::error::CoreResult;

/// Single read-only API for accessing the ThreadLog.
///
/// Every consumer -- WS display, LLM prompt builder, policy gates, headless CLI --
/// reads the audit log through this trait. Compile-time read-only: no write methods.
#[async_trait]
pub trait ThreadLogReader: Send + Sync {
    async fn get_log(&self, thread_id: &str) -> CoreResult<ThreadLog>;
    async fn get_step_count(&self, thread_id: &str) -> CoreResult<usize>;
    async fn list_thread_ids(&self) -> Vec<String>;
}
