use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Draft,
    Approved,
    Completed,
    Cancelled,
}

impl Default for PlanStatus {
    fn default() -> Self {
        PlanStatus::Draft
    }
}

impl PlanStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlanStatus::Draft => "draft",
            PlanStatus::Approved => "approved",
            PlanStatus::Completed => "completed",
            PlanStatus::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, PlanStatus::Completed | PlanStatus::Cancelled)
    }

    pub fn try_approve(self) -> Result<PlanStatus, String> {
        match self {
            PlanStatus::Draft => Ok(PlanStatus::Approved),
            other => Err(format!("cannot approve plan in {:?} state", other)),
        }
    }

    pub fn try_complete(self) -> Result<PlanStatus, String> {
        match self {
            PlanStatus::Approved => Ok(PlanStatus::Completed),
            other => Err(format!("cannot complete plan in {:?} state", other)),
        }
    }

    pub fn try_cancel(self) -> Result<PlanStatus, String> {
        if self.is_terminal() {
            return Err(format!("cannot cancel plan in terminal {:?} state", self));
        }
        Ok(PlanStatus::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
    /// Task was completed but requires correction / updates before the plan can proceed.
    NeedsUpdate,
}

impl Default for TaskStatus {
    fn default() -> Self {
        TaskStatus::Pending
    }
}

/// Trait abstracting over cleanse/model task types so `Plan<T>` can be generic.
pub trait PlanTask: Clone + std::fmt::Debug + Serialize + serde::de::DeserializeOwned + Send + Sync {
    fn task_id(&self) -> &str;
    fn expected_model_path(&self) -> Option<&str>;
    fn status(&self) -> TaskStatus;
    fn set_status(&mut self, status: TaskStatus);
    fn checklist(&self) -> &[PlanChecklistItem];
    fn checklist_mut(&mut self) -> &mut Vec<PlanChecklistItem>;
    fn invariants(&self) -> &[String];
    /// Prefix used when generating canonical work_group IDs (e.g. "cleanse", "model").
    fn work_group_prefix() -> &'static str;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanProgress {
    /// Thread step index cursor: when updating from the thread log, only consider steps after this
    /// index (exclusive). This keeps progress updates idempotent and cheap.
    #[serde(default)]
    pub last_applied_step_idx: usize,

    /// Consecutive failed batch attempts (resets on a fully successful attempt).
    /// Used to avoid infinite remediation loops in plan-batched authoring.
    #[serde(default)]
    pub consecutive_batch_failures: usize,

    /// Total failed batch attempts across the plan lifetime (diagnostics only).
    #[serde(default)]
    pub total_batch_failures: usize,
}

impl Default for PlanProgress {
    fn default() -> Self {
        Self {
            last_applied_step_idx: 0,
            consecutive_batch_failures: 0,
            total_batch_failures: 0,
        }
    }
}

/// Audit trail for plan mutations (repairs, pruning, canonicalization).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanMutation {
    pub ts: String,
    pub reason_code: MutationReasonCode,
    /// Additional structured detail (best-effort; keep small).
    #[serde(default)]
    pub detail: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecklistOrigin {
    Initial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecklistItemStatus {
    Pending,
    InProgress,
    Done,
    Blocked,
    NeedsUpdate,
}

impl Default for ChecklistItemStatus {
    fn default() -> Self {
        ChecklistItemStatus::Pending
    }
}

// TODO(item-91): Convert ChecklistEvidence.kind from bare String to a typed enum once all
// concrete kind values are catalogued. Values are currently passed as free-form strings via
// plan_progress::make_checklist_evidence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistEvidence {
    pub kind: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_id: Option<String>,
    pub step_idx: usize,
    #[serde(default)]
    pub ts: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanChecklistItem {
    pub checklist_item_id: String,
    pub label: String,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub status: ChecklistItemStatus,
    pub origin: ChecklistOrigin,
    #[serde(default)]
    pub evidence: Vec<ChecklistEvidence>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkGroupKind {
    AuthorSql,
    AuthorSchema,
    Validate,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkGroupItemRef {
    pub task_id: String,
    pub checklist_item_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanWorkGroup {
    pub group_id: String,
    pub label: String,
    pub kind: WorkGroupKind,
    pub items: Vec<WorkGroupItemRef>,
    #[serde(default)]
    pub depends_on_group_ids: Option<Vec<String>>,
}

// TODO(item-90): Convert MutationReasonCode from a string newtype to an enum with known
// variants + `Other(String)` fallback once the concrete reason codes are stabilized.
// Currently only constructed via deserialization; audit JSON payloads for known values first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MutationReasonCode(pub String);

// -----------------------
// Design-first plan spec
// -----------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFolder {
    Marts,
    Core,
}

impl Default for ModelFolder {
    fn default() -> Self {
        ModelFolder::Marts
    }
}

impl ModelFolder {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelFolder::Marts => "marts",
            ModelFolder::Core => "core",
        }
    }
}

impl std::fmt::Display for ModelFolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

impl std::fmt::Display for JoinType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JoinType::Inner => f.write_str("inner"),
            JoinType::Left => f.write_str("left"),
            JoinType::Right => f.write_str("right"),
            JoinType::Full => f.write_str("full"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    ManyToOne,
    OneToMany,
    OneToOne,
    ManyToMany,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Raw,
    Clean,
    Derived,
    QualityFlag,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputFieldSpec {
    /// Output column name.
    pub name: String,
    pub kind: FieldKind,
    /// Upstream source columns (or prior-stage columns) this field depends on.
    #[serde(default)]
    pub source_columns: Vec<String>,
    /// A concise, imperative expression or transformation contract.
    /// This is not required to be dialect-perfect SQL; it is the design contract that authoring
    /// should implement faithfully.
    pub expression: String,
    /// Optional intended type (best-effort). Prefer empty over guessing.
    #[serde(default)]
    pub data_type: Option<String>,
    /// Whether the field is allowed to be NULL in the output.
    #[serde(default)]
    pub nullable: bool,
    /// Optional one-line meaning / usage guidance.
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CleanseImplementationSpec {
    /// Schema version for the spec itself (not the overall plan). Enables future evolution.
    pub spec_version: i64,
    /// Row-preserving is mandatory for SILVER (models/staging/).
    pub row_preserving: bool,
    /// Explicit output field design for this staging model.
    pub output_fields: Vec<OutputFieldSpec>,
    /// Optional explicit guardrails to prevent accidental grain enforcement (e.g. "no filtering", "no dedup").
    #[serde(default)]
    pub prohibited_ops: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JoinSpec {
    pub right_model: String,
    pub join_type: JoinType,
    pub on: Vec<String>,
    #[serde(default)]
    pub cardinality: Option<Cardinality>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricSpec {
    pub name: String,
    /// One-line definition (formula + inclusion/exclusion rules).
    pub definition: String,
    /// Optional caveats/assumptions (bounded).
    #[serde(default)]
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelImplementationSpec {
    pub spec_version: i64,
    /// Grain contract, e.g. "1 row per order_id".
    pub grain: String,
    /// Inputs should align with task.inputs (stg_* only).
    #[serde(default)]
    pub inputs: Vec<String>,
    /// Join contract for composing the model.
    #[serde(default)]
    pub joins: Vec<JoinSpec>,
    /// Metric definitions for business use.
    #[serde(default)]
    pub metrics: Vec<MetricSpec>,
    /// Expected output schema contract (columns + semantics).
    #[serde(default)]
    pub output_fields: Vec<OutputFieldSpec>,
    /// Assumptions that require validation probes before downstream reliance.
    #[serde(default)]
    pub assumptions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanseTask {
    pub dataset_id: String,
    /// Expected DBT model file path for this dataset (project-relative).
    /// Typically models/staging/stg_<schema>_<table>.sql
    #[serde(default)]
    pub expected_model_path: Option<String>,
    /// High-signal planning output: inferred grain, keys, time fields, and any known hazards.
    /// This should be grounded in schema + stats + sample probes.
    #[serde(default)]
    pub invariants: Vec<String>,
    /// Design-first implementation contract for this staging model.
    /// `None` means the spec has not yet been enriched from the skeleton plan.
    #[serde(default)]
    pub implementation_spec: Option<CleanseImplementationSpec>,
    #[serde(default)]
    pub status: TaskStatus,
    #[serde(default)]
    pub checklist: Vec<PlanChecklistItem>,
}

impl PlanTask for CleanseTask {
    fn task_id(&self) -> &str { &self.dataset_id }
    fn expected_model_path(&self) -> Option<&str> { self.expected_model_path.as_deref() }
    fn status(&self) -> TaskStatus { self.status }
    fn set_status(&mut self, status: TaskStatus) { self.status = status; }
    fn checklist(&self) -> &[PlanChecklistItem] { &self.checklist }
    fn checklist_mut(&mut self) -> &mut Vec<PlanChecklistItem> { &mut self.checklist }
    fn invariants(&self) -> &[String] { &self.invariants }
    fn work_group_prefix() -> &'static str { "cleanse" }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelTask {
    pub name: String,
    #[serde(default)]
    pub folder: ModelFolder,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub inputs: Vec<String>,
    /// Expected DBT model file path for this model (project-relative).
    #[serde(default)]
    pub expected_model_path: Option<String>,
    /// High-signal planning output: grain, join keys, time semantics, uniqueness expectations.
    #[serde(default)]
    pub invariants: Vec<String>,
    /// Design-first implementation contract for this model.
    /// `None` means the spec has not yet been enriched from the skeleton plan.
    #[serde(default)]
    pub implementation_spec: Option<ModelImplementationSpec>,
    #[serde(default)]
    pub status: TaskStatus,
    #[serde(default)]
    pub checklist: Vec<PlanChecklistItem>,
}

impl PlanTask for ModelTask {
    fn task_id(&self) -> &str { &self.name }
    fn expected_model_path(&self) -> Option<&str> { self.expected_model_path.as_deref() }
    fn status(&self) -> TaskStatus { self.status }
    fn set_status(&mut self, status: TaskStatus) { self.status = status; }
    fn checklist(&self) -> &[PlanChecklistItem] { &self.checklist }
    fn checklist_mut(&mut self) -> &mut Vec<PlanChecklistItem> { &mut self.checklist }
    fn invariants(&self) -> &[String] { &self.invariants }
    fn work_group_prefix() -> &'static str { "model" }
}

// ---------- Generic Plan ----------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(
    serialize = "T: serde::Serialize",
    deserialize = "T: serde::de::DeserializeOwned"
))]
pub struct Plan<T: PlanTask> {
    #[serde(default)]
    pub plan_key: String,
    pub status: PlanStatus,
    #[serde(default)]
    pub project_snapshot: Value,
    pub tasks: Vec<T>,
    pub batches: Vec<Vec<String>>,
    #[serde(default)]
    pub work_groups: Vec<PlanWorkGroup>,
    #[serde(default)]
    pub mutations: Vec<PlanMutation>,
    #[serde(default)]
    pub progress: PlanProgress,
}

impl<T: PlanTask> Plan<T> {
    /// Regenerate work_groups from the current batches. Must be called after any
    /// mutation that adds/removes tasks or batches to maintain internal consistency.
    pub fn reconcile_work_groups(&mut self) {
        self.work_groups = crate::plan_progress::canonical_work_groups_from_batches(
            &self.batches,
            T::work_group_prefix(),
        );
    }
}

pub type CleansePlan = Plan<CleanseTask>;
pub type ModelPlan = Plan<ModelTask>;

// ---------- TrackPlan ----------

pub trait TrackPlan {
    fn plan_key(&self) -> &str;
    fn status(&self) -> PlanStatus;
    fn set_status(&mut self, status: PlanStatus);
    fn tasks_len(&self) -> usize;
    fn batches_len(&self) -> usize;
    fn progress_mut(&mut self) -> &mut PlanProgress;
    fn executable_plan_issues(&self) -> Vec<String>;
    fn is_empty(&self) -> bool {
        self.tasks_len() == 0 || self.batches_len() == 0
    }

    fn approve(&mut self) -> Result<(), String> {
        let next = self.status().try_approve()?;
        self.set_status(next);
        Ok(())
    }

    fn complete(&mut self) -> Result<(), String> {
        let next = self.status().try_complete()?;
        self.set_status(next);
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), String> {
        let next = self.status().try_cancel()?;
        self.set_status(next);
        Ok(())
    }
}

impl<T: PlanTask> TrackPlan for Plan<T> {
    fn plan_key(&self) -> &str { &self.plan_key }
    fn status(&self) -> PlanStatus { self.status }
    fn set_status(&mut self, status: PlanStatus) { self.status = status; }
    fn tasks_len(&self) -> usize { self.tasks.len() }
    fn batches_len(&self) -> usize { self.batches.len() }
    fn progress_mut(&mut self) -> &mut PlanProgress { &mut self.progress }
    fn executable_plan_issues(&self) -> Vec<String> {
        crate::plan_progress::executable_plan_issues(self)
    }
}

// ---------- Grounded / Persistable newtypes ----------

#[derive(Clone, Debug)]
pub struct GroundedPlan<T: PlanTask>(pub(crate) Plan<T>);

pub type GroundedCleansePlan = GroundedPlan<CleanseTask>;
pub type GroundedModelPlan = GroundedPlan<ModelTask>;

impl<T: PlanTask> GroundedPlan<T> {
    pub fn into_inner(self) -> Plan<T> {
        self.0
    }
}

#[derive(Clone, Debug)]
pub enum PersistablePlan<T: PlanTask> {
    Grounded(GroundedPlan<T>),
    Terminal(Plan<T>),
}

pub type PersistableCleansePlan = PersistablePlan<CleanseTask>;
pub type PersistableModelPlan = PersistablePlan<ModelTask>;

impl<T: PlanTask> PersistablePlan<T> {
    pub fn into_inner(self) -> Plan<T> {
        match self {
            Self::Grounded(v) => v.into_inner(),
            Self::Terminal(v) => v,
        }
    }
}
