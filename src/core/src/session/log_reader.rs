use async_trait::async_trait;

use crate::error::CoreResult;
use super::{ThreadEvent, ThreadLog};

/// Single read-only API for accessing the ThreadLog.
///
/// Every consumer -- WS display, LLM prompt builder, policy gates, headless CLI --
/// reads the audit log through this trait. Compile-time read-only: no write methods.
#[async_trait]
pub trait ThreadLogReader: Send + Sync {
    async fn get_log(&self, thread_id: &str) -> CoreResult<ThreadLog>;
    async fn get_events(&self, thread_id: &str, max: usize) -> CoreResult<Vec<ThreadEvent>>;
    async fn get_step_count(&self, thread_id: &str) -> CoreResult<usize>;
    async fn list_thread_ids(&self) -> Vec<String>;
}
