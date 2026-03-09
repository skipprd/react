use crate::control_flow::Phase;
use crate::failure_kind::FailureKind;
use crate::plan;
use react_core::session::ThreadStore;

pub const MAX_CONSECUTIVE_BATCH_FAILURES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryBudget {
    pub used: usize,
    pub limit: usize,
}

impl RetryBudget {
    pub fn exhausted(self) -> bool {
        self.used >= self.limit
    }
}

pub fn subjective_retry_limit() -> usize {
    crate::env_util::env_usize(
        crate::env_util::env_keys::AGENT_MAX_SUBJECTIVE_RETRIES,
    )
    .unwrap_or(2)
    .max(1)
    .min(6)
}

pub fn subjective_retry_state_cap() -> usize {
    // ExecutionState stores the first attempt as `1`, while callers compare with `> limit`.
    // Keep the +1 cap so persisted counters and guard behavior stay deterministic.
    subjective_retry_limit().saturating_add(1)
}

pub fn batch_budget(progress: &plan::PlanProgress) -> RetryBudget {
    RetryBudget {
        used: progress.consecutive_batch_failures,
        limit: MAX_CONSECUTIVE_BATCH_FAILURES,
    }
}

pub fn note_batch_result(progress: &mut plan::PlanProgress, ok: bool) -> RetryBudget {
    if ok {
        progress.consecutive_batch_failures = 0;
    } else {
        progress.consecutive_batch_failures = progress.consecutive_batch_failures.saturating_add(1);
        progress.total_batch_failures = progress.total_batch_failures.saturating_add(1);
    }
    batch_budget(progress)
}

pub fn note_batch_result_with_failure_kind(
    progress: &mut plan::PlanProgress,
    ok: bool,
    failure_kind: Option<FailureKind>,
) -> RetryBudget {
    if ok {
        return note_batch_result(progress, true);
    }
    if matches!(failure_kind, Some(FailureKind::InfraTransient)) {
        // Transient upstream outages must not consume deterministic batch-lock budget.
        return batch_budget(progress);
    }
    note_batch_result(progress, false)
}

pub(crate) enum SubjectiveRetryOutcome {
    WithinBudget(usize),
    Exhausted(usize),
}

/// Bump the subjective retry counter for `kind` and return whether the budget is exhausted.
pub(crate) async fn check_subjective_retry_budget(
    thread_store: &ThreadStore,
    thread_id: &str,
    kind: crate::progress_controller::SubjectiveRetryKind,
) -> Result<SubjectiveRetryOutcome, String> {
    let cap = subjective_retry_state_cap();
    let mut st = crate::progress_controller::ExecutionState::load(
        &thread_store.control_store(),
        thread_id,
    )
    .await
    .map_err(|e| format!("failed to load execution state for subjective retry: {e}"))?
    .unwrap_or_else(crate::progress_controller::ExecutionState::new);
    let retries = st.bump_subjective_retry(kind, cap);
    st.save(&thread_store.control_store(), thread_id)
        .await
        .map_err(|e| format!("failed to persist subjective retry: {e}"))?;
    if retries > subjective_retry_limit() {
        Ok(SubjectiveRetryOutcome::Exhausted(retries))
    } else {
        Ok(SubjectiveRetryOutcome::WithinBudget(retries))
    }
}

/// Clear subjective retry counters matching a predicate. Persists immediately.
pub(crate) async fn clear_subjective_retries_matching(
    thread_store: &ThreadStore,
    thread_id: &str,
    f: impl Fn(&crate::progress_controller::SubjectiveRetryKind) -> bool,
) -> Result<(), String> {
    let mut st = crate::progress_controller::ExecutionState::load(
        &thread_store.control_store(),
        thread_id,
    )
    .await
    .map_err(|e| format!("failed to load execution state for retry reset: {e}"))?
    .unwrap_or_else(crate::progress_controller::ExecutionState::new);
    st.clear_subjective_retries_matching(f);
    st.save(&thread_store.control_store(), thread_id)
        .await
        .map_err(|e| format!("failed to clear subjective retries: {e}"))
}

/// Apply a guard-block, commit a loopback decision to the corresponding author phase,
/// and return `PhaseExecutorOutcome::StayInPhase`.
pub(crate) async fn guard_block_loopback_to_author(
    thread_store: &ThreadStore,
    thread_id: &str,
    phase: Phase,
    guard_kind: crate::domain_types::GuardBlockKind,
    reason: String,
    reason_code: crate::domain_types::PhaseReasonCode,
    detail: Option<serde_json::Value>,
) -> Result<super::PhaseExecutorOutcome, String> {
    crate::phase_contract::commit_guard_block(
        thread_store,
        thread_id,
        phase,
        guard_kind,
        reason,
    )
    .await?;
    let to_phase = if phase == Phase::CleanseValidate {
        Phase::CleanseAuthor
    } else {
        Phase::ModelAuthor
    };
    crate::phase_contract::commit_phase_decision(
        thread_store,
        thread_id,
        Some(phase),
        crate::phase_contract::PhaseDecision::loopback(to_phase, Some(reason_code), detail),
    )
    .await?;
    Ok(super::PhaseExecutorOutcome::StayInPhase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_batch_result_updates_consecutive_and_total() {
        let mut progress = plan::PlanProgress::default();
        let b1 = note_batch_result(&mut progress, false);
        assert_eq!(b1.used, 1);
        assert_eq!(progress.total_batch_failures, 1);

        let b2 = note_batch_result(&mut progress, false);
        assert_eq!(b2.used, 2);
        assert_eq!(progress.total_batch_failures, 2);

        let b3 = note_batch_result(&mut progress, true);
        assert_eq!(b3.used, 0);
        assert_eq!(progress.total_batch_failures, 2);
    }

    #[test]
    fn infra_transient_failure_does_not_consume_budget() {
        let mut progress = plan::PlanProgress::default();
        let budget = note_batch_result_with_failure_kind(
            &mut progress,
            false,
            Some(FailureKind::InfraTransient),
        );
        assert_eq!(budget.used, 0);
        assert_eq!(progress.total_batch_failures, 0);
    }
}
