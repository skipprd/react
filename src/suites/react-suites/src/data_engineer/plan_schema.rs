use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn strict_schema_for<T: JsonSchema>() -> Result<Value, String> {
    react_core::schema_registry::strict_json_schema_for::<T>().map_err(|e| e.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CleansePlanSkeletonTaskV1 {
    pub dataset_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CleansePlanSkeletonV1 {
    #[schemars(length(min = 1))]
    pub tasks: Vec<CleansePlanSkeletonTaskV1>,
    pub batches: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelPlanSkeletonTaskV1 {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelPlanSkeletonV1 {
    pub tasks: Vec<ModelPlanSkeletonTaskV1>,
    pub batches: Vec<Vec<String>>,
}

// Re-export canonical plan types that now derive JsonSchema directly.
// These aliases preserve backwards compatibility with existing V1 references.
pub type FieldKindV1 = super::plan_types::FieldKind;
pub type OutputFieldSpecV1 = super::plan_types::OutputFieldSpec;
pub type CleanseImplementationSpecV1 = super::plan_types::CleanseImplementationSpec;
pub type JoinTypeV1 = super::plan_types::JoinType;
pub type CardinalityV1 = super::plan_types::Cardinality;
pub type JoinSpecV1 = super::plan_types::JoinSpec;
pub type MetricSpecV1 = super::plan_types::MetricSpec;
pub type ModelImplementationSpecV1 = super::plan_types::ModelImplementationSpec;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CleansePlanEnrichmentItemV1 {
    pub task_id: String,
    pub implementation_spec: CleanseImplementationSpecV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CleansePlanEnrichmentV1 {
    pub items: Vec<CleansePlanEnrichmentItemV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelPlanEnrichmentItemV1 {
    pub task_id: String,
    pub implementation_spec: ModelImplementationSpecV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelPlanEnrichmentV1 {
    pub items: Vec<ModelPlanEnrichmentItemV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelPlanCandidateV1 {
    pub name: String,
    pub insight: String,
    pub observation: String,
    pub value_score: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelPlanCandidatesV1 {
    pub candidates: Vec<ModelPlanCandidateV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanDesignBlockerV1 {
    pub code: PlanDesignBlockerCodeV1,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub severity: Option<Severity>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanDesignFixV1 {
    pub action: PlanDesignFixActionV1,
    #[serde(default)]
    pub blocker_code: Option<PlanDesignBlockerCodeV1>,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanDesignBlockerCodeV1 {
    MissingGroundedTasks,
    MissingTaskSpecs,
    MissingWorkGroupCoverage,
    InvalidChecklistProgress,
    Other,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanDesignFixActionV1 {
    RegenerateTasks,
    EnrichTaskSpecs,
    RepairWorkGroups,
    RepairChecklistCoverage,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanDesignCritiqueV1 {
    pub ok: bool,
    #[serde(default)]
    pub blockers: Vec<PlanDesignBlockerV1>,
    #[serde(default)]
    pub fixes: Vec<PlanDesignFixV1>,
}
