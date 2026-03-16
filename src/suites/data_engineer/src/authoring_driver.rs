use crate::control_flow::Phase;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthoringKind {
    Cleanse,
    Model,
}

#[derive(Clone, Debug)]
pub(crate) struct AuthoringCtx {
    pub phase: Phase,
}

#[derive(Clone, Debug)]
pub(crate) enum AuthoringTurnResult {
    Continue,
    HardError { message: String },
}

pub(crate) trait AuthoringPlanAdapter {
    fn kind(&self) -> AuthoringKind;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CleanseAdapter;

impl AuthoringPlanAdapter for CleanseAdapter {
    fn kind(&self) -> AuthoringKind {
        AuthoringKind::Cleanse
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ModelAdapter;

impl AuthoringPlanAdapter for ModelAdapter {
    fn kind(&self) -> AuthoringKind {
        AuthoringKind::Model
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AuthoringAdapter {
    Cleanse(CleanseAdapter),
    Model(ModelAdapter),
}

impl AuthoringAdapter {
    pub(crate) fn kind(self) -> AuthoringKind {
        match self {
            Self::Cleanse(v) => v.kind(),
            Self::Model(v) => v.kind(),
        }
    }
}

pub(crate) fn adapter_for_phase(phase: Phase) -> Option<AuthoringAdapter> {
    match phase {
        Phase::CleanseAuthor => Some(AuthoringAdapter::Cleanse(CleanseAdapter)),
        Phase::ModelAuthor => Some(AuthoringAdapter::Model(ModelAdapter)),
        _ => None,
    }
}

pub(crate) struct AuthoringDriver;

impl AuthoringDriver {
    pub(crate) fn run_turn(
        ctx: &AuthoringCtx,
        snapshot: &crate::progress_controller::AuthoringProgressSnapshot,
    ) -> AuthoringTurnResult {
        Self::stepboundary_outcome(ctx, snapshot)
    }

    pub(crate) fn stepboundary_outcome(
        ctx: &AuthoringCtx,
        snapshot: &crate::progress_controller::AuthoringProgressSnapshot,
    ) -> AuthoringTurnResult {
        if !snapshot.progress_made
            && snapshot.reason
                == Some(crate::progress_controller::AuthoringNoProgressReason::NoMutationProgress)
        {
            return AuthoringTurnResult::HardError {
                message: format!(
                    "authoring stall: no mutating file operation observed after repeated step-boundary attempts (phase={})",
                    ctx.phase.as_str()
                ),
            };
        }
        AuthoringTurnResult::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepboundary_returns_hard_error_for_no_progress_in_hard_repair() {
        let ctx = AuthoringCtx {
            phase: Phase::ModelAuthor,
        };
        let snapshot = crate::progress_controller::AuthoringProgressSnapshot {
            progress_made: false,
            reason: Some(
                crate::progress_controller::AuthoringNoProgressReason::NoMutationProgress,
            ),
        };
        let out = AuthoringDriver::stepboundary_outcome(&ctx, &snapshot);
        assert!(matches!(out, AuthoringTurnResult::HardError { .. }));
    }
}
