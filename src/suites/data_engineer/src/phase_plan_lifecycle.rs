use react_core::agent::AgentCtx;

use crate::plan;
use crate::plan_types::{CleansePlan, ModelPlan, PlanStatus, TrackPlan};
use crate::track_spec::TrackKind;

/// Enum wrapper allowing a single variable to hold either plan type while
/// still exposing the `TrackPlan` trait. Pattern matching on the variants
/// is used by callers that need access to the concrete plan (e.g. phase_plan,
/// plan_review_helpers). This wrapper is the price of having two distinct
/// task types; the macro below keeps the delegation boilerplate-free.
#[derive(Clone)]
pub(super) enum TrackPlanDoc {
    Cleanse(CleansePlan),
    Model(ModelPlan),
}

macro_rules! delegate_track_plan {
    ($self:ident, $method:ident $(, $arg:ident : $ty:ty)*) => {
        match $self {
            Self::Cleanse(p) => p.$method($($arg),*),
            Self::Model(p) => p.$method($($arg),*),
        }
    };
}

impl TrackPlan for TrackPlanDoc {
    fn plan_key(&self) -> &str { delegate_track_plan!(self, plan_key) }
    fn status(&self) -> PlanStatus { delegate_track_plan!(self, status) }
    fn set_status(&mut self, status: PlanStatus) { delegate_track_plan!(self, set_status, status: PlanStatus) }
    fn tasks_len(&self) -> usize { delegate_track_plan!(self, tasks_len) }
    fn batches_len(&self) -> usize { delegate_track_plan!(self, batches_len) }
    fn progress_mut(&mut self) -> &mut crate::plan_types::PlanProgress { delegate_track_plan!(self, progress_mut) }
    fn executable_plan_issues(&self) -> Vec<String> { delegate_track_plan!(self, executable_plan_issues) }
}

pub(super) async fn load_plan_for_track(
    actx: &AgentCtx,
    track: TrackKind,
) -> Option<TrackPlanDoc> {
    match track {
        TrackKind::Cleanse => plan::load_cleanse_plan(actx).await.ok().flatten().map(TrackPlanDoc::Cleanse),
        TrackKind::Model => plan::load_model_plan(actx).await.ok().flatten().map(TrackPlanDoc::Model),
    }
}

pub(super) async fn save_plan(actx: &AgentCtx, plan: &TrackPlanDoc) -> Result<(), String> {
    match plan {
        TrackPlanDoc::Cleanse(plan) => crate::plan::save_cleanse_plan(actx, plan).await.map_err(|e| e.to_string()),
        TrackPlanDoc::Model(plan) => crate::plan::save_model_plan(actx, plan).await.map_err(|e| e.to_string()),
    }
}
