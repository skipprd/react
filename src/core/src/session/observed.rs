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

struct ToolEndPanicGuard {
    thread_store: ThreadStore,
    thread_id: String,
    thread_step: Option<ThreadStep>,
    tool_name: String,
    phase: String,
    start_logged: bool,
    armed: bool,
}

impl ToolEndPanicGuard {
    fn new(
        thread_store: ThreadStore,
        thread_id: String,
        thread_step: ThreadStep,
        tool_name: String,
        phase: String,
        start_logged: bool,
    ) -> Self {
        Self {
            thread_store,
            thread_id,
            thread_step: Some(thread_step),
            tool_name,
            phase,
            start_logged,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
        self.thread_step = None;
    }
}

impl Drop for ToolEndPanicGuard {
    fn drop(&mut self) {
        if !self.armed || !self.start_logged || !std::thread::panicking() {
            return;
        }
        let Some(step) = self.thread_step.take() else {
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(
                "run_observed: panic finalizer missing runtime handle tool={} thread_id={} phase={}",
                self.tool_name,
                self.thread_id,
                self.phase
            );
            tracing::warn!(
                "observability_gap kind=unmatched_tool_start tool={} thread_id={} phase={}",
                self.tool_name,
                self.thread_id,
                self.phase
            );
            return;
        };
        let store = self.thread_store.clone();
        let thread_id = self.thread_id.clone();
        let tool_name = self.tool_name.clone();
        let phase = self.phase.clone();
        handle.spawn(async move {
            if let Err(e) = store.append_step(&thread_id, step).await {
                tracing::warn!(
                    "run_observed: panic finalizer failed to append tool_end name={} thread_id={} phase={}: {}",
                    tool_name,
                    thread_id,
                    phase,
                    e
                );
                tracing::warn!(
                    "observability_gap kind=unmatched_tool_start tool={} thread_id={} phase={}",
                    tool_name,
                    thread_id,
                    phase
                );
            }
        });
    }
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

        let mut panic_guard = ToolEndPanicGuard::new(
            self.clone(),
            thread_id.to_string(),
            ThreadStep::ToolEnd {
                tool_id: tool_id.clone(),
                name: meta.name.clone(),
                clean_name: meta.clean_name.clone(),
                args: meta.args.clone(),
                status: ToolStepStatus::Failed,
                payload: None,
                ctx: meta.ctx.clone(),
                observation: ToolObservation::normalize(serde_json::json!({
                    "ok": false,
                    "errors": [format!("tool '{}' panicked before producing a normal observation", meta.name)],
                })),
                ts: chrono::Utc::now().to_rfc3339(),
                agent: meta.agent.clone(),
            },
            meta.name.clone(),
            meta.phase.clone(),
            start_logged,
        );

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
        panic_guard.disarm();

        return_result
    }
}
