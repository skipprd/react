use react_core::agent::AgentCtx;

use crate::data_engineer::plan;
use crate::data_engineer::plan_types::{CleansePlan, ModelPlan, PlanStatus, TrackPlan};
use crate::data_engineer::track_spec::TrackKind;

#[derive(Clone)]
pub(super) enum TrackPlanDoc {
    Cleanse(CleansePlan),
    Model(ModelPlan),
}

impl TrackPlan for TrackPlanDoc {
    fn plan_key(&self) -> &str {
        match self {
            Self::Cleanse(p) => p.plan_key(),
            Self::Model(p) => p.plan_key(),
        }
    }
    fn status(&self) -> PlanStatus {
        match self {
            Self::Cleanse(p) => p.status(),
            Self::Model(p) => p.status(),
        }
    }
    fn set_status(&mut self, status: PlanStatus) {
        match self {
            Self::Cleanse(p) => p.set_status(status),
            Self::Model(p) => p.set_status(status),
        }
    }
    fn tasks_len(&self) -> usize {
        match self {
            Self::Cleanse(p) => p.tasks_len(),
            Self::Model(p) => p.tasks_len(),
        }
    }
    fn batches_len(&self) -> usize {
        match self {
            Self::Cleanse(p) => p.batches_len(),
            Self::Model(p) => p.batches_len(),
        }
    }
    fn progress_mut(&mut self) -> &mut crate::data_engineer::plan_types::PlanProgress {
        match self {
            Self::Cleanse(p) => p.progress_mut(),
            Self::Model(p) => p.progress_mut(),
        }
    }
}

pub(super) async fn load_plan_for_track(
    actx: &AgentCtx,
    track: TrackKind,
) -> Option<TrackPlanDoc> {
    match track {
        TrackKind::Cleanse => plan::load_cleanse_plan(actx).await.map(TrackPlanDoc::Cleanse),
        TrackKind::Model => plan::load_model_plan(actx).await.map(TrackPlanDoc::Model),
    }
}

pub(super) async fn save_plan(actx: &AgentCtx, plan: &TrackPlanDoc) -> Result<(), String> {
    match plan {
        TrackPlanDoc::Cleanse(plan) => crate::data_engineer::plan::save_cleanse_plan(actx, plan).await,
        TrackPlanDoc::Model(plan) => crate::data_engineer::plan::save_model_plan(actx, plan).await,
    }
}
