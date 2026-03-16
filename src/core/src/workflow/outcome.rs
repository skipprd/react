use crate::suite::FlowFrame;

/// Outcome of a single phase execution turn.
///
/// Returned by `PhaseExecutor::execute_turn` to tell the workflow runner
/// what happened and how to adjust budgets / continue.
pub enum PhaseOutcome {
    /// Phase is still active and this turn made durable forward progress.
    StayedWithProgress { detail: String },
    /// Phase is still active but is explicitly waiting on more work/signal.
    StayedWaiting { reason: String },
    /// A phase transition was committed; reload state and continue.
    TransitionCommitted,
    /// Workflow complete; return these frames to the caller.
    Return(Vec<FlowFrame>),
    /// Phase execution failed; error has been recorded by the executor.
    Failed { reason: String },
}

impl PhaseOutcome {
    pub fn stayed_with_progress(detail: impl Into<String>) -> Self {
        Self::StayedWithProgress {
            detail: detail.into(),
        }
    }

    pub fn stayed_waiting(reason: impl Into<String>) -> Self {
        Self::StayedWaiting {
            reason: reason.into(),
        }
    }
}
