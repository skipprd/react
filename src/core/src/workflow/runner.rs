use async_trait::async_trait;

use super::outcome::PhaseOutcome;
use crate::suite::FlowFrame;

/// Configuration for the workflow orchestration loop.
pub struct Config {
    pub max_phase_steps: usize,
    pub max_consecutive_waiting_idle: usize,
    pub max_consecutive_waiting_active: usize,
    /// Hard ceiling on total iterations regardless of progress resets.
    /// Prevents infinite loops when `StayedWithProgress` is returned repeatedly.
    pub max_total_steps: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_phase_steps: 50,
            max_consecutive_waiting_idle: 5,
            max_consecutive_waiting_active: 12,
            max_total_steps: 500,
        }
    }
}

/// Trait that suites implement to drive their phase logic.
///
/// The workflow runner calls `execute_turn` in a loop, handling budget
/// management and idle detection generically. The executor is responsible
/// for all domain-specific work: loading state, evaluating guards,
/// dispatching to the correct phase, and recording errors.
#[async_trait]
pub trait PhaseExecutor: Send + Sync {
    /// Execute one turn of the workflow.
    ///
    /// The executor should load state, evaluate guards, dispatch to the
    /// appropriate phase, and return the outcome. Append any intermediate
    /// frames to `out_frames`.
    async fn execute_turn(&self, out_frames: &mut Vec<FlowFrame>) -> PhaseOutcome;

    /// Called when the step budget is exhausted without completion.
    /// The executor should mark its state as failed and perform any cleanup.
    async fn on_budget_exhausted(&self, out_frames: &mut Vec<FlowFrame>, total_steps: usize);

    /// Current step count in the thread log (used for idle detection).
    async fn step_count(&self) -> usize;
}

/// Generic workflow orchestration loop.
///
/// Repeatedly calls `executor.execute_turn()` and manages:
/// - Per-progress step budget (reset on `StayedWithProgress`)
/// - Idle detection via consecutive `StayedWaiting` outcomes
/// - Budget exhaustion (delegates to `executor.on_budget_exhausted`)
pub async fn run(
    executor: &(dyn PhaseExecutor + '_),
    config: &Config,
) -> Result<Vec<FlowFrame>, String> {
    let max_phase_steps = config.max_phase_steps;
    let mut remaining_steps = max_phase_steps;
    let mut total_steps: usize = 0;
    let mut consecutive_waiting: usize = 0;
    let mut out_frames: Vec<FlowFrame> = Vec::new();

    while remaining_steps > 0 {
        total_steps += 1;
        remaining_steps = remaining_steps.saturating_sub(1);

        if total_steps > config.max_total_steps {
            executor
                .on_budget_exhausted(&mut out_frames, total_steps)
                .await;
            return Err(format!(
                "headless_hard_ceiling: absolute step limit reached.\n\nBudget:\n- max_total_steps={}\n- total_steps={total_steps}",
                config.max_total_steps,
            ));
        }

        let step_count_before = executor.step_count().await;

        match executor.execute_turn(&mut out_frames).await {
            PhaseOutcome::StayedWithProgress { detail } => {
                tracing::debug!(phase_progress = %detail);
                remaining_steps = max_phase_steps;
                consecutive_waiting = 0;
            }
            PhaseOutcome::StayedWaiting { reason } => {
                remaining_steps += 1;
                let step_count_after = executor.step_count().await;
                let was_active = step_count_after > step_count_before + 2;
                if was_active {
                    consecutive_waiting += 1;
                } else {
                    consecutive_waiting += 2;
                }
                let limit = if was_active {
                    config.max_consecutive_waiting_active
                } else {
                    config.max_consecutive_waiting_idle
                };
                tracing::debug!(
                    phase_waiting = %reason,
                    consecutive_waiting,
                    was_active,
                    limit,
                );
                if consecutive_waiting >= limit {
                    tracing::error!(
                        consecutive_waiting,
                        "phase returned StayedWaiting {consecutive_waiting} times consecutively without progress",
                    );
                    return Err(format!(
                        "phase stuck: returned StayedWaiting {consecutive_waiting} times consecutively without progress (last reason: {reason})",
                    ));
                }
            }
            PhaseOutcome::TransitionCommitted => {
                consecutive_waiting = 0;
            }
            PhaseOutcome::Return(frames) => return Ok(frames),
            PhaseOutcome::Failed { reason } => return Err(reason),
        }
    }

    executor
        .on_budget_exhausted(&mut out_frames, total_steps)
        .await;
    Err(format!(
        "headless_budget_exhausted: Agent reached the phase-step budget without completing.\n\nBudget:\n- max_steps_per_progress={max_phase_steps}\n- total_steps={total_steps}",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    struct MockExecutor {
        outcomes: Mutex<Vec<PhaseOutcome>>,
        step_counts: Mutex<Vec<usize>>,
        budget_exhausted_called: AtomicUsize,
    }

    impl MockExecutor {
        fn new(outcomes: Vec<PhaseOutcome>, step_counts: Vec<usize>) -> Self {
            Self {
                outcomes: Mutex::new(outcomes),
                step_counts: Mutex::new(step_counts),
                budget_exhausted_called: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl PhaseExecutor for MockExecutor {
        async fn execute_turn(&self, _out_frames: &mut Vec<FlowFrame>) -> PhaseOutcome {
            let mut outcomes = self.outcomes.lock().unwrap();
            if outcomes.is_empty() {
                PhaseOutcome::Failed {
                    reason: "no more outcomes".to_string(),
                }
            } else {
                outcomes.remove(0)
            }
        }

        async fn on_budget_exhausted(&self, _out_frames: &mut Vec<FlowFrame>, _total_steps: usize) {
            self.budget_exhausted_called.fetch_add(1, Ordering::Relaxed);
        }

        async fn step_count(&self) -> usize {
            let mut counts = self.step_counts.lock().unwrap();
            if counts.is_empty() {
                0
            } else {
                counts.remove(0)
            }
        }
    }

    #[tokio::test]
    async fn return_outcome_terminates_immediately() {
        let exec = MockExecutor::new(
            vec![
                PhaseOutcome::TransitionCommitted,
                PhaseOutcome::Return(vec![]),
            ],
            vec![],
        );
        let config = Config {
            max_phase_steps: 10,
            ..Default::default()
        };
        let result = run(&exec, &config).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_outcome_returns_error() {
        let exec = MockExecutor::new(
            vec![PhaseOutcome::Failed {
                reason: "boom".to_string(),
            }],
            vec![],
        );
        let config = Config::default();
        let result = run(&exec, &config).await;
        assert_eq!(result.unwrap_err(), "boom");
    }

    #[tokio::test]
    async fn budget_exhaustion_calls_handler() {
        let exec = MockExecutor::new(
            vec![
                PhaseOutcome::TransitionCommitted,
                PhaseOutcome::TransitionCommitted,
                PhaseOutcome::TransitionCommitted,
            ],
            vec![],
        );
        let config = Config {
            max_phase_steps: 3,
            ..Default::default()
        };
        let result = run(&exec, &config).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("budget_exhausted"));
        assert_eq!(exec.budget_exhausted_called.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn stayed_with_progress_resets_budget() {
        let exec = MockExecutor::new(
            vec![
                PhaseOutcome::TransitionCommitted,
                PhaseOutcome::TransitionCommitted,
                PhaseOutcome::stayed_with_progress("progress"),
                PhaseOutcome::TransitionCommitted,
                PhaseOutcome::TransitionCommitted,
                PhaseOutcome::Return(vec![]),
            ],
            vec![],
        );
        let config = Config {
            max_phase_steps: 3,
            ..Default::default()
        };
        let result = run(&exec, &config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn consecutive_idle_waiting_triggers_error() {
        let exec = MockExecutor::new(
            vec![
                PhaseOutcome::stayed_waiting("wait1"),
                PhaseOutcome::stayed_waiting("wait2"),
                PhaseOutcome::stayed_waiting("wait3"),
            ],
            // step_count returns same value each time (idle)
            vec![0, 0, 0, 0, 0, 0],
        );
        let config = Config {
            max_phase_steps: 100,
            max_consecutive_waiting_idle: 4,
            max_consecutive_waiting_active: 12,
            ..Default::default()
        };
        let result = run(&exec, &config).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("StayedWaiting"));
        assert!(err.contains("without progress"));
    }

    #[tokio::test]
    async fn active_waiting_has_higher_threshold() {
        let mut outcomes = Vec::new();
        let mut step_counts = Vec::new();
        // 5 active waits (each iteration bumps step count by 3+)
        for i in 0..5 {
            outcomes.push(PhaseOutcome::stayed_waiting(format!("active wait {i}")));
            step_counts.push(i * 10); // before
            step_counts.push((i + 1) * 10); // after (big jump = active)
        }
        outcomes.push(PhaseOutcome::Return(vec![]));

        let exec = MockExecutor::new(outcomes, step_counts);
        let config = Config {
            max_phase_steps: 100,
            max_consecutive_waiting_idle: 2,
            max_consecutive_waiting_active: 10,
            ..Default::default()
        };
        let result = run(&exec, &config).await;
        assert!(result.is_ok());
    }
}
