use std::future::Future;

use super::{ExecutionContext, ThreadStep, ThreadStore, ToolObservation, ToolStepStatus};

/// Metadata describing a tool step for paired ToolStart/ToolEnd logging.
///
/// All tool call sites MUST use [`ThreadStore::run_observed`] rather than
/// constructing `ThreadStep::ToolStart` / `ThreadStep::ToolEnd` directly.
/// This ensures the paired-event invariant is enforced in one place.
#[derive(Clone, Debug)]
pub struct ToolStepMeta {
    pub agent: String,
    /// Logical phase label for observability (e.g. "agent_loop", "cleanse_validate").
    pub phase: String,
    /// Machine-readable tool name (e.g. "dbt_validate", "run_sql").
    pub name: String,
    /// Human-readable short label for UI (e.g. "Validate DBT", "Run SQL").
    pub clean_name: String,
    pub args: serde_json::Value,
    pub ctx: Option<ExecutionContext>,
}

impl ThreadStore {
    /// Execute `operation` inside a paired ToolStart/ToolEnd bracket.
    ///
    /// **Guarantees:**
    /// - A `ToolStart` step is appended before the operation runs.
    /// - A `ToolEnd` step is **always** appended, even if the operation errors.
    /// - Append failures are logged with structured observability-gap markers so
    ///   monitoring can alert on unmatched events.
    ///
    /// `to_observation` converts a successful operation result `&T` into a raw
    /// JSON value that is then normalised via [`ToolObservation::normalize`].
    /// If `to_observation` itself returns `Err`, a failed observation is recorded
    /// and the error is propagated as the return value.
    pub async fn run_observed<T, F, Fut, Obs>(
        &self,
        thread_id: &str,
        meta: ToolStepMeta,
        operation: F,
        to_observation: Obs,
    ) -> Result<T, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, String>>,
        Obs: FnOnce(&T) -> Result<serde_json::Value, String>,
    {
        let tool_id = uuid::Uuid::new_v4().to_string();

        let start_logged = match self
            .append_step(
                thread_id,
                ThreadStep::ToolStart {
                    tool_id: tool_id.clone(),
                    name: meta.name.clone(),
                    clean_name: meta.clean_name.clone(),
                    args: meta.args.clone(),
                    status: ToolStepStatus::Running,
                    payload: None,
                    ctx: meta.ctx.clone(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: meta.agent.clone(),
                },
            )
            .await
        {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!(
                    "run_observed: failed to append tool_start name={} thread_id={} phase={}: {}",
                    meta.name,
                    thread_id,
                    meta.phase,
                    e
                );
                false
            }
        };

        let result = operation().await;

        let (status, obs, return_result) = match result {
            Ok(value) => match to_observation(&value) {
                Ok(raw) => {
                    let obs = ToolObservation::normalize(raw);
                    let status = if obs.ok {
                        ToolStepStatus::Ok
                    } else {
                        ToolStepStatus::Failed
                    };
                    (status, obs, Ok(value))
                }
                Err(e) => {
                    let msg = format!("failed to build observation for {}: {}", meta.name, e);
                    (
                        ToolStepStatus::Failed,
                        ToolObservation::normalize(serde_json::json!({"ok": false, "errors": [msg.clone()]})),
                        Err(msg),
                    )
                }
            },
            Err(e) => {
                let obs = ToolObservation::normalize(
                    serde_json::json!({"ok": false, "errors": [e.clone()]}),
                );
                (ToolStepStatus::Failed, obs, Err(e))
            }
        };

        let payload = obs
            .extra
            .get("payload")
            .cloned()
            .or_else(|| obs.extra.get("ui_payload").cloned());

        if let Err(e) = self
            .append_step(
                thread_id,
                ThreadStep::ToolEnd {
                    tool_id,
                    name: meta.name.clone(),
                    clean_name: meta.clean_name.clone(),
                    args: meta.args,
                    status,
                    payload,
                    ctx: meta.ctx,
                    observation: obs,
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: meta.agent.clone(),
                },
            )
            .await
        {
            tracing::warn!(
                "run_observed: failed to append tool_end name={} thread_id={} phase={}: {}",
                meta.name,
                thread_id,
                meta.phase,
                e
            );
            if start_logged {
                tracing::warn!(
                    "observability_gap kind=unmatched_tool_start tool={} thread_id={} phase={}",
                    meta.name,
                    thread_id,
                    meta.phase
                );
            }
        }

        return_result
    }
}
