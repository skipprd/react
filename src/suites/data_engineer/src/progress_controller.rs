use crate::failure_kind::FailureKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fmt;

use crate::domain_types::PhaseReasonCode;
use react_core::session::ControlStateStore;

use crate::control_flow::Phase;

pub const EXECUTION_STATE_SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_MAX_STALL_COUNT: usize = 3;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FailedModelRef {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub materialization: RepairTargetMaterialization,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepairTargetMaterialization {
    Existing,
    Missing,
    #[default]
    Unknown,
}

/// Single source of truth for error/failure context injected into LLM prompts.
///
/// Built from `ExecutionState`; every phase executor receives this instead of
/// ad-hoc `(Option<String>, Vec<FailedModelRef>)` tuples. The `format_error_context`
/// method guarantees that error text is NEVER dropped regardless of whether
/// specific failing models are identified.
/// Typed plan data for a single failing task, carried into repair prompts so
/// the repair LLM sees the exact contract it must satisfy.
#[derive(Clone, Debug)]
pub struct TaskRepairSpec {
    pub task_id: String,
    pub source_schema: Vec<crate::plan_types::SourceColumnDef>,
    pub implementation_spec_json: String,
}

#[derive(Clone, Debug, Default)]
pub struct RepairPromptContext {
    pub brief: Option<String>,
    pub failed_models: Vec<FailedModelRef>,
    pub stall_count: usize,
    pub attempt_count: usize,
    pub failure_signature: Option<String>,
    /// Suite-level guard note (e.g. from authoring budget exhaustion).
    /// Separate from `brief` which is the dbt validation output.
    pub guard_note: Option<String>,
    /// Pre-fetched file contents for failing models (path → content).
    /// Populated asynchronously before prompt rendering; never serialized.
    pub target_contents: Vec<(String, String)>,
    /// Recent failed mutating file operations from this thread so the repair
    /// prompt can explicitly steer away from repeating the same broken patch.
    pub recent_failed_file_ops: Vec<RecentFailedFileOp>,
    /// Typed plan data for failing tasks so the repair LLM sees the
    /// authoritative contract (schema + implementation spec) from the plan.
    pub task_specs: Vec<TaskRepairSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentFailedFileOp {
    pub op: String,
    pub path: String,
    pub error_brief: String,
}

impl RepairPromptContext {
    /// Format all available error context for injection into an LLM prompt.
    /// Always includes the brief (error text) AND per-model targets independently.
    /// Escalates framing when stall_count indicates repeated failure.
    pub fn format_error_context(&self) -> String {
        let mut out = String::new();
        if !self.failed_models.is_empty() {
            out.push_str("Failing model targets:\n");
            for fm in self.failed_models.iter().take(6) {
                let name = if fm.name.trim().is_empty() {
                    "unknown_model"
                } else {
                    fm.name.as_str()
                };
                let file = if fm.file.trim().is_empty() {
                    "(unknown file)"
                } else {
                    fm.file.as_str()
                };
                out.push_str(&format!("- {name} ({file})"));
                match fm.materialization {
                    RepairTargetMaterialization::Existing => {
                        out.push_str(" [target_exists]");
                    }
                    RepairTargetMaterialization::Missing => {
                        out.push_str(" [target_missing]");
                    }
                    RepairTargetMaterialization::Unknown => {}
                }
                if let Some(ref err) = fm.error {
                    let trimmed = err.trim();
                    if !trimmed.is_empty() {
                        out.push_str(&format!(" — {trimmed}"));
                    }
                }
                out.push('\n');
            }
        }
        if let Some(ref brief) = self.brief {
            let trimmed = brief.trim();
            if !trimmed.is_empty() {
                out.push_str("\nLast dbt_validate summary:\n");
                out.push_str(trimmed);
                out.push('\n');
            }
        }
        if let Some(ref note) = self.guard_note {
            let trimmed = note.trim();
            if !trimmed.is_empty() {
                out.push_str("\nSuite guard note (must resolve before validate):\n");
                out.push_str(trimmed);
                out.push('\n');
            }
        }
        if !self.recent_failed_file_ops.is_empty() {
            out.push_str("\nRecent failed file mutations (do NOT repeat these verbatim):\n");
            for item in self.recent_failed_file_ops.iter().take(3) {
                out.push_str(&format!(
                    "- file op='{}' path='{}' failed: {}\n",
                    item.op,
                    item.path,
                    item.error_brief
                ));
            }
            out.push_str(
                "If a prior patch failed, read the exact current file content and make a materially different edit.\n",
            );
        }
        if self.stall_count >= 2 {
            out.push_str(&format!(
                "\nWARNING: This repair has stalled for {} consecutive iterations \
                 with the same error. Previous patches did NOT resolve the issue. \
                 Read the error above carefully and make a DIFFERENT change.\n",
                self.stall_count,
            ));
        }
        if !self.task_specs.is_empty() {
            out.push_str("\n--- Plan contract for failing tasks (AUTHORITATIVE) ---\n");
            for spec in &self.task_specs {
                out.push_str(&format!("\n### Task: {}\n", spec.task_id));
                if !spec.source_schema.is_empty() {
                    out.push_str("Source columns:\n");
                    for c in &spec.source_schema {
                        out.push_str(&format!("  - {} ({})\n", c.name, c.data_type));
                    }
                }
                if !spec.implementation_spec_json.is_empty() {
                    out.push_str("Implementation spec:\n```json\n");
                    out.push_str(&spec.implementation_spec_json);
                    if !spec.implementation_spec_json.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str("```\n");
                }
            }
        }
        if !self.target_contents.is_empty() {
            out.push_str("\n--- Current file contents (use these as exact patch context) ---\n");
            for (path, content) in &self.target_contents {
                let fence = if path.ends_with(".yml") || path.ends_with(".yaml") {
                    "yaml"
                } else {
                    "sql"
                };
                out.push_str(&format!("\n### {path}\n```{fence}\n"));
                out.push_str(content);
                if !content.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n");
            }
        }
        out
    }

    pub fn has_context(&self) -> bool {
        self.brief.is_some()
            || !self.failed_models.is_empty()
            || self.guard_note.is_some()
            || !self.recent_failed_file_ops.is_empty()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LastValidateState {
    #[serde(default)]
    pub step_idx: Option<usize>,
    #[serde(default)]
    pub ts: Option<String>,
    #[serde(default)]
    pub ok: Option<bool>,
    #[serde(default)]
    pub compile_ok: Option<bool>,
    #[serde(default)]
    pub run_ok: Option<bool>,
    #[serde(default)]
    pub brief: Option<String>,
    #[serde(default)]
    pub failed_models: Vec<FailedModelRef>,
    #[serde(default)]
    pub failure_class: Option<FailureKind>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublishPlanState {
    #[serde(default)]
    pub pending_plan_sha256: Option<String>,
    #[serde(default)]
    pub pending_set_ts: Option<String>,
    #[serde(default)]
    pub last_published_plan_sha256: Option<String>,
    #[serde(default)]
    pub published_ts: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFocusState {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub dataset_id: Option<String>,
    #[serde(default)]
    pub exists: bool,
    #[serde(default)]
    pub ts: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LastMutationSummary {
    pub op: MutationOp,
    #[serde(default)]
    pub affected_paths: Vec<String>,
    #[serde(default)]
    pub select_terms: Vec<String>,
    #[serde(default)]
    pub ts: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MutationOp {
    Patch,
    Move,
    Remove,
}

impl Default for MutationOp {
    fn default() -> Self {
        Self::Patch
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTier {
    Unknown,
    Cleanse,
    Model,
}

impl Default for ExecutionTier {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Discover,
    Mutate,
    Validate,
    Done,
    Failed,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        Self::Discover
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepairLadderStep {
    PatchTarget = 1,
    ReplaceContents = 2,
    FsOp = 3,
    Stop = 4,
}

impl Default for RepairLadderStep {
    fn default() -> Self {
        Self::PatchTarget
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepairType {
    Unknown,
    Schema,
    SqlTarget,
}

impl Default for RepairType {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct SqlModelPath(String);

impl SqlModelPath {
    pub fn parse(path: impl Into<String>) -> Result<Self, String> {
        let path = path.into();
        if !is_sql_model_path(path.as_str()) {
            return Err(format!(
                "invalid SqlModelPath '{}': expected models/*.sql path",
                path
            ));
        }
        Ok(Self(path))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for SqlModelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_str())
    }
}

impl TryFrom<String> for SqlModelPath {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<SqlModelPath> for String {
    fn from(value: SqlModelPath) -> Self {
        value.0
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct SchemaDocPath(String);

impl SchemaDocPath {
    pub fn parse(path: impl Into<String>) -> Result<Self, String> {
        let path = path.into();
        if !is_schema_doc_path(path.as_str()) {
            return Err(format!(
                "invalid SchemaDocPath '{}': expected models/*.yml|yaml path",
                path
            ));
        }
        Ok(Self(path))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl TryFrom<String> for SchemaDocPath {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<SchemaDocPath> for String {
    fn from(value: SchemaDocPath) -> Self {
        value.0
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(try_from = "String", into = "String")]
pub enum RepairTargetPath {
    SqlModel(SqlModelPath),
    SchemaDoc(SchemaDocPath),
}

impl RepairTargetPath {
    pub fn parse(path: impl Into<String>) -> Result<Self, String> {
        let path = path.into();
        if let Ok(sql) = SqlModelPath::parse(path.clone()) {
            return Ok(Self::SqlModel(sql));
        }
        if let Ok(schema) = SchemaDocPath::parse(path.clone()) {
            return Ok(Self::SchemaDoc(schema));
        }
        Err(format!(
            "invalid RepairTargetPath '{}': expected models/*.sql or models/*.yml|yaml path",
            path
        ))
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::SqlModel(path) => path.as_str(),
            Self::SchemaDoc(path) => path.as_str(),
        }
    }
}

impl fmt::Display for RepairTargetPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<String> for RepairTargetPath {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<RepairTargetPath> for String {
    fn from(value: RepairTargetPath) -> Self {
        match value {
            RepairTargetPath::SqlModel(path) => path.into(),
            RepairTargetPath::SchemaDoc(path) => path.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct RepairModeCore {
    #[serde(default)]
    pub ladder_step: RepairLadderStep,
    #[serde(default)]
    pub attempt_count: usize,
    #[serde(default)]
    pub repair_started_mutation_epoch: Option<u64>,
    #[serde(default)]
    pub consecutive_noop_patches: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchemaRepairMode {
    #[serde(flatten)]
    pub core: RepairModeCore,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SqlTargetRepairMode {
    pub target_path: SqlModelPath,
    #[serde(default)]
    pub materialization: RepairTargetMaterialization,
    #[serde(flatten)]
    pub core: RepairModeCore,
}

impl Default for SchemaRepairMode {
    fn default() -> Self {
        Self {
            core: RepairModeCore::default(),
        }
    }
}

impl SchemaRepairMode {
    pub fn ladder_step(&self) -> RepairLadderStep {
        self.core.ladder_step.clone()
    }
    pub fn attempt_count(&self) -> usize {
        self.core.attempt_count
    }
    pub fn consecutive_noop_patches(&self) -> usize {
        self.core.consecutive_noop_patches
    }
}

impl SqlTargetRepairMode {
    pub fn for_existing_target(target_path: SqlModelPath) -> Self {
        Self {
            target_path,
            materialization: RepairTargetMaterialization::Existing,
            core: RepairModeCore::default(),
        }
    }
    pub fn for_missing_target(target_path: SqlModelPath) -> Self {
        Self {
            target_path,
            materialization: RepairTargetMaterialization::Missing,
            core: RepairModeCore {
                ladder_step: RepairLadderStep::ReplaceContents,
                ..RepairModeCore::default()
            },
        }
    }
    pub fn ladder_step(&self) -> RepairLadderStep {
        self.core.ladder_step.clone()
    }
    pub fn attempt_count(&self) -> usize {
        self.core.attempt_count
    }
    pub fn consecutive_noop_patches(&self) -> usize {
        self.core.consecutive_noop_patches
    }
    pub fn note_materialized(&mut self) {
        self.materialization = RepairTargetMaterialization::Existing;
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepairModeState {
    Inactive,
    Schema(SchemaRepairMode),
    SqlTarget(SqlTargetRepairMode),
}

impl Default for RepairModeState {
    fn default() -> Self {
        Self::Inactive
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProbeOutcomeKind {
    MeaningfulNewSignal,
    MeaningfulSameSignal,
    NonMeaningful,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProbeRequirementStatus {
    NotRequired,
    Required,
    Allowed,
    ExhaustedRequireMutation,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProbeSignature {
    #[serde(default)]
    pub normalized_sql: String,
    #[serde(default)]
    pub row_count: usize,
    #[serde(default)]
    pub header_count: usize,
    #[serde(default)]
    pub first_row_fingerprint: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProbeState {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub attempts_total: usize,
    #[serde(default)]
    pub meaningful_attempts: usize,
    #[serde(default)]
    pub repeated_signature_streak: usize,
    #[serde(default)]
    pub non_meaningful_attempts: usize,
    #[serde(default)]
    pub failed_attempts: usize,
    #[serde(default)]
    pub last_signature: Option<ProbeSignature>,
}

impl ProbeSignature {
    pub fn from_run_sql(sql: &str, observation: &Value) -> Self {
        if let Some(p) = observation.get("probe") {
            let normalized_sql = p
                .get("normalized_sql")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    sql.split_whitespace()
                        .collect::<Vec<&str>>()
                        .join(" ")
                        .to_ascii_lowercase()
                });
            let row_count = p
                .get("row_count")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(0);
            let header_count = p
                .get("header_count")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(0);
            let first_row_fingerprint = p
                .get("first_row_fingerprint")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            return Self {
                normalized_sql,
                row_count,
                header_count,
                first_row_fingerprint,
            };
        }
        let normalized_sql = sql
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join(" ")
            .to_ascii_lowercase();
        let header_count = observation
            .get("header")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let rows = observation
            .get("rows")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let row_count = rows.len();
        let first_row_fingerprint = rows.first().and_then(|row| {
            row.as_array().map(|arr| {
                let preview: Vec<&Value> = arr.iter().take(6).collect();
                serde_json::to_string(&preview).unwrap_or_default()
            })
        });
        Self {
            normalized_sql,
            row_count,
            header_count,
            first_row_fingerprint,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RepairTarget {
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub path: Option<RepairTargetPath>,
    #[serde(default)]
    pub error_class: Option<FailureKind>,
    #[serde(default)]
    pub materialization: RepairTargetMaterialization,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FailureSignature {
    pub class: FailureKind,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub canonical_path: Option<RepairTargetPath>,
    #[serde(default)]
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RepairIntent {
    SqlPatch {
        target: SqlModelPath,
        backlog: Vec<RepairTarget>,
    },
    SqlRewrite {
        target: SqlModelPath,
        backlog: Vec<RepairTarget>,
    },
    Schema {
        backlog: Vec<RepairTarget>,
    },
    NoRepair {
        reason: NoRepairReason,
        backlog: Vec<RepairTarget>,
    },
}

impl RepairIntent {
    pub fn backlog(&self) -> &[RepairTarget] {
        match self {
            Self::SqlPatch { backlog, .. } => backlog.as_slice(),
            Self::SqlRewrite { backlog, .. } => backlog.as_slice(),
            Self::Schema { backlog } => backlog.as_slice(),
            Self::NoRepair { backlog, .. } => backlog.as_slice(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoRepairReason {
    FailureClassNotRepairable,
    NoRepairableTarget,
    MissingTargetMaterialization,
    MultiTargetSqlFailure,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProgressDelta {
    #[serde(default)]
    pub target_hash_changed: bool,
    #[serde(default)]
    pub failed_target_count_delta: i64,
    #[serde(default)]
    pub failure_signature_changed: bool,
    #[serde(default)]
    pub checklist_completed_delta: i64,
    #[serde(default)]
    pub progress_made: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthoringNoProgressReason {
    NoMutationProgress,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoringProgressSnapshot {
    pub progress_made: bool,
    pub reason: Option<AuthoringNoProgressReason>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SubjectiveRetryKind {
    PlanSemanticInvalid,
    PlanGroundingEmptyAfterPrune,
    PlanGroundingStagingDiscoveryEmpty,
    ReviewPatchImpl,
    ValidatePrecheckFailed,
    ValidateExecutionFailed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PublishApprovalDecision {
    AwaitingUserApproval,
    Approved,
    Rejected,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublishApprovalState {
    pub decision: PublishApprovalDecision,
    pub ts: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PublishRetryKind {
    AwaitApprovalLoop,
    PublishFailureLoop,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublishRetryState {
    pub kind: PublishRetryKind,
    pub count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlanViolation {
    pub detecting_phase: Phase,
    pub task_id: Option<String>,
    pub evidence: String,
}

impl PlanViolation {
    pub fn new(
        detecting_phase: Phase,
        task_id: Option<String>,
        evidence: impl Into<String>,
    ) -> Self {
        Self {
            detecting_phase,
            task_id,
            evidence: evidence.into(),
        }
    }
}

pub fn format_plan_violations(violations: &[PlanViolation]) -> String {
    if violations.is_empty() {
        return String::new();
    }
    let mut out = String::from("PLAN REVISION REQUIRED — downstream phases reported the following issues with the current plan:\n\n");
    for (i, v) in violations.iter().enumerate() {
        out.push_str(&format!("Issue {}:\n", i + 1));
        out.push_str(&format!("  Detected by: {}\n", v.detecting_phase.as_str()));
        if let Some(ref tid) = v.task_id {
            out.push_str(&format!("  Plan task: {}\n", tid));
        }
        out.push_str(&format!("  Evidence: {}\n\n", v.evidence.trim()));
    }
    out.push_str(
        "Revise the plan to fix these issues. Do NOT repeat the same unachievable instructions.\n",
    );
    out
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PatchImplIntent {
    pub phase: Phase,
    pub entry_mutation_epoch: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlanRevisionIntent {
    pub violations: Vec<PlanViolation>,
    #[serde(default)]
    pub strategy: PlanRevisionStrategy,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanRevisionStrategy {
    Rewrite,
    Amend,
}

impl Default for PlanRevisionStrategy {
    fn default() -> Self {
        Self::Rewrite
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ManifestLookupPathKind {
    CanonicalTarget,
    Ambiguous,
    NonCanonical,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ManifestLookupFailureKind {
    NoSuchKey,
    PointerNotFound,
}

impl ManifestLookupFailureKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::NoSuchKey => "NoSuchKey",
            Self::PointerNotFound => "PointerNotFound",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManifestLookupState {
    #[serde(default)]
    pub retry_suppressed: bool,
    #[serde(default)]
    pub failure_signature: Option<String>,
    #[serde(default)]
    pub repeated_failure_count: usize,
    #[serde(default)]
    pub canonical_success_count: usize,
    #[serde(default)]
    pub noncanonical_attempt_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionState {
    pub schema_version: u32,
    #[serde(default)]
    pub phase: PhaseState,
    #[serde(default)]
    pub repair: RepairState,
    #[serde(default)]
    pub publish: PublishState,
    #[serde(default)]
    pub manifest: ManifestState,
    #[serde(default)]
    pub telemetry: TelemetryState,
    #[serde(default)]
    pub subjective_retries: BTreeMap<SubjectiveRetryKind, usize>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TelemetryState {
    #[serde(default)]
    pub last_validate: Option<LastValidateState>,
    #[serde(default)]
    pub probe: ProbeState,
    #[serde(default)]
    pub artifact_focus: Option<ArtifactFocusState>,
    #[serde(default)]
    pub last_mutation_summary: Option<LastMutationSummary>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PhaseState {
    #[serde(default)]
    pub current_phase: Option<Phase>,
    #[serde(default)]
    pub phase_reason_code: Option<PhaseReasonCode>,
    #[serde(default)]
    pub phase_reason_detail: Option<Value>,
    #[serde(default)]
    pub replan_backtracks: usize,
    #[serde(default)]
    pub current_tier: ExecutionTier,
    #[serde(default)]
    pub mode: ExecutionMode,
    #[serde(default)]
    pub pending_plan_revision: Option<PlanRevisionIntent>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RepairState {
    #[serde(default)]
    pub last_failure_signature: Option<FailureSignature>,
    #[serde(default)]
    pub repair_backlog: Vec<RepairTarget>,
    #[serde(default)]
    pub repair_mode: RepairModeState,
    #[serde(default)]
    pub mutation_epoch: u64,
    #[serde(default)]
    pub stall_count: usize,
    #[serde(default)]
    pub last_progress_delta: Option<ProgressDelta>,
    #[serde(default)]
    pub last_error_brief: Option<String>,
    #[serde(default)]
    pub pending_patch_impl: Option<PatchImplIntent>,
    #[serde(default)]
    pub last_batch_infra_transient: bool,
}

impl RepairState {
    pub fn hard_mutation_repair_mode(&self) -> bool {
        !matches!(self.repair_mode, RepairModeState::Inactive)
    }

    pub fn repair_type(&self) -> RepairType {
        match &self.repair_mode {
            RepairModeState::Inactive => RepairType::Unknown,
            RepairModeState::Schema(_) => RepairType::Schema,
            RepairModeState::SqlTarget(_) => RepairType::SqlTarget,
        }
    }

    pub fn single_target_repair_path(&self) -> Option<&str> {
        match &self.repair_mode {
            RepairModeState::Inactive => None,
            RepairModeState::Schema(_) => None,
            RepairModeState::SqlTarget(mode) => Some(mode.target_path.as_str()),
        }
    }

    pub fn take_batch_infra_transient(&mut self) -> bool {
        std::mem::take(&mut self.last_batch_infra_transient)
    }

    fn core(&self) -> Option<&RepairModeCore> {
        match &self.repair_mode {
            RepairModeState::Inactive => None,
            RepairModeState::Schema(mode) => Some(&mode.core),
            RepairModeState::SqlTarget(mode) => Some(&mode.core),
        }
    }

    fn core_mut(&mut self) -> Option<&mut RepairModeCore> {
        match &mut self.repair_mode {
            RepairModeState::Inactive => None,
            RepairModeState::Schema(mode) => Some(&mut mode.core),
            RepairModeState::SqlTarget(mode) => Some(&mut mode.core),
        }
    }

    pub fn ladder_step(&self) -> RepairLadderStep {
        self.core()
            .map(|c| c.ladder_step.clone())
            .unwrap_or(RepairLadderStep::PatchTarget)
    }

    pub fn attempt_count(&self) -> usize {
        self.core().map(|c| c.attempt_count).unwrap_or(0)
    }

    pub fn consecutive_noop_patches(&self) -> usize {
        self.core().map(|c| c.consecutive_noop_patches).unwrap_or(0)
    }

    pub fn ensure_target_path(&mut self, path: SqlModelPath) {
        if let RepairModeState::SqlTarget(mode) = &mut self.repair_mode {
            mode.target_path = path;
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PublishState {
    #[serde(default)]
    pub publish_approval: Option<PublishApprovalState>,
    #[serde(default)]
    pub publish_retries: Vec<PublishRetryState>,
    #[serde(default)]
    pub publish_plan: PublishPlanState,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManifestState {
    #[serde(default)]
    pub manifest_lookup: ManifestLookupState,
    #[serde(default)]
    pub plan_bootstrap: PlanBootstrapState,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlanBootstrapState {
    #[serde(default)]
    pub cleanse_done: bool,
    #[serde(default)]
    pub model_done: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct WorkflowControlState {
    phase: PhaseState,
    repair: RepairState,
    publish: PublishState,
    probe: ProbeState,
}

impl WorkflowControlState {
    fn mark_validate_success(&mut self, tier: ExecutionTier) {
        self.phase.current_tier = tier;
        self.phase.mode = ExecutionMode::Done;
        self.phase.pending_plan_revision = None;

        self.repair.last_failure_signature = None;
        self.repair.repair_backlog.clear();
        self.repair.repair_mode = RepairModeState::Inactive;
        self.repair.stall_count = 0;
        self.repair.last_progress_delta = Some(ProgressDelta {
            progress_made: true,
            ..ProgressDelta::default()
        });
        self.repair.last_error_brief = None;
        self.repair.pending_patch_impl = None;

        self.publish.publish_approval = None;
        self.publish.publish_retries.clear();

        self.probe = ProbeState::default();
        self.probe.required = false;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DataEngineerEvent {
    ValidatePassed {
        tier: ExecutionTier,
    },
    ValidateFailed {
        tier: ExecutionTier,
        failure_class: FailureKind,
        failure_signature: FailureSignature,
        repair_intent: RepairIntent,
        brief: Option<String>,
        compile_ok: Option<bool>,
        run_ok: Option<bool>,
    },
    BatchAuthoringFailed {
        tier: ExecutionTier,
        kind: FailureKind,
        failed_targets: Vec<FailedModelRef>,
        brief: String,
    },
    BatchAuthoringRecovered,
}

impl ExecutionState {
    pub fn repair_prompt_context(&self) -> RepairPromptContext {
        let (brief, failed_models) = if let Some(ref lv) = self.telemetry.last_validate {
            let b = lv
                .brief
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            (b, lv.failed_models.clone())
        } else {
            (None, Vec::new())
        };
        let guard_note = self
            .repair
            .last_error_brief
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        RepairPromptContext {
            brief,
            failed_models,
            stall_count: self.repair.stall_count,
            attempt_count: self.attempt_count(),
            failure_signature: self
                .repair
                .last_failure_signature
                .as_ref()
                .map(|s| format!("{:?}", s)),
            guard_note,
            target_contents: Vec::new(),
            recent_failed_file_ops: Vec::new(),
            task_specs: Vec::new(),
        }
    }

    fn last_validate_failed(&self) -> bool {
        self.telemetry.last_validate.as_ref().and_then(|lv| lv.ok) == Some(false)
    }

    /// Convenience delegation to `self.repair_state()`. These exist so callers
    /// can query repair properties without knowing about the `RepairState` layer.
    pub fn hard_mutation_repair_mode(&self) -> bool {
        self.repair_state().hard_mutation_repair_mode()
    }
    pub fn repair_type(&self) -> RepairType {
        self.repair_state().repair_type()
    }
    pub fn ladder_step(&self) -> RepairLadderStep {
        self.repair_state().ladder_step()
    }
    pub fn attempt_count(&self) -> usize {
        self.repair_state().attempt_count()
    }
    pub fn consecutive_noop_patches(&self) -> usize {
        self.repair_state().consecutive_noop_patches()
    }
    pub fn single_target_repair_path(&self) -> Option<String> {
        self.repair_state()
            .single_target_repair_path()
            .map(ToString::to_string)
    }

    pub fn ensure_repair_target_path(&mut self, path: SqlModelPath) {
        self.with_repair_state_mut(|repair| {
            repair.ensure_target_path(path);
        });
    }

    fn disable_repair_mode(repair: &mut RepairState) {
        repair.repair_mode = RepairModeState::Inactive;
    }

    fn enable_repair_mode(repair: &mut RepairState, repair_intent: &RepairIntent) {
        repair.repair_mode = match repair_intent {
            RepairIntent::Schema { .. } => RepairModeState::Schema(SchemaRepairMode {
                core: RepairModeCore {
                    repair_started_mutation_epoch: Some(repair.mutation_epoch),
                    ..RepairModeCore::default()
                },
            }),
            RepairIntent::SqlPatch { target, .. } => {
                let mut mode = SqlTargetRepairMode::for_existing_target(target.clone());
                mode.core.repair_started_mutation_epoch = Some(repair.mutation_epoch);
                RepairModeState::SqlTarget(mode)
            }
            RepairIntent::SqlRewrite { target, .. } => {
                let mut mode = SqlTargetRepairMode::for_missing_target(target.clone());
                mode.core.repair_started_mutation_epoch = Some(repair.mutation_epoch);
                RepairModeState::SqlTarget(mode)
            }
            RepairIntent::NoRepair { .. } => RepairModeState::Inactive,
        };
    }

    pub fn new() -> Self {
        Self {
            schema_version: EXECUTION_STATE_SCHEMA_VERSION,
            ..Default::default()
        }
    }

    pub fn apply_validate_success(&mut self, tier: ExecutionTier) {
        self.telemetry.last_validate = Some(LastValidateState {
            ts: Some(chrono::Utc::now().to_rfc3339()),
            ok: Some(true),
            compile_ok: Some(true),
            run_ok: Some(true),
            ..LastValidateState::default()
        });
        self.clear_subjective_retry_kind(SubjectiveRetryKind::ValidatePrecheckFailed);
        self.clear_subjective_retry_kind(SubjectiveRetryKind::ValidateExecutionFailed);
        self.with_workflow_control_state_mut(|state| {
            state.mark_validate_success(tier);
        });
    }

    pub fn phase_state(&self) -> &PhaseState {
        &self.phase
    }

    pub(crate) fn with_phase_state_mut(&mut self, mutate: impl FnOnce(&mut PhaseState)) {
        mutate(&mut self.phase);
        self.debug_assert_invariants();
    }

    pub fn repair_state(&self) -> &RepairState {
        &self.repair
    }

    fn with_repair_state_mut(&mut self, mutate: impl FnOnce(&mut RepairState)) {
        mutate(&mut self.repair);
        self.debug_assert_invariants();
    }

    pub fn publish_state(&self) -> &PublishState {
        &self.publish
    }

    fn with_publish_state_mut(&mut self, mutate: impl FnOnce(&mut PublishState)) {
        mutate(&mut self.publish);
        self.debug_assert_invariants();
    }

    pub fn probe_state_snapshot(&self) -> ProbeState {
        self.telemetry.probe.clone()
    }

    pub fn set_probe_state_snapshot(&mut self, probe: ProbeState) {
        self.telemetry.probe = probe;
        self.debug_assert_invariants();
    }

    fn with_probe_state_mut(&mut self, mutate: impl FnOnce(&mut ProbeState)) {
        let mut probe = self.probe_state_snapshot();
        mutate(&mut probe);
        self.set_probe_state_snapshot(probe);
    }

    fn workflow_control_state(&self) -> WorkflowControlState {
        WorkflowControlState {
            phase: self.phase.clone(),
            repair: self.repair.clone(),
            publish: self.publish.clone(),
            probe: self.probe_state_snapshot(),
        }
    }

    fn set_workflow_control_state(&mut self, state: WorkflowControlState) {
        self.phase = state.phase;
        self.repair = state.repair;
        self.publish = state.publish;
        self.telemetry.probe = state.probe;
        self.debug_assert_invariants();
    }

    fn with_workflow_control_state_mut(&mut self, mutate: impl FnOnce(&mut WorkflowControlState)) {
        let mut state = self.workflow_control_state();
        mutate(&mut state);
        self.set_workflow_control_state(state);
    }

    pub fn manifest_state(&self) -> &ManifestState {
        &self.manifest
    }

    fn with_manifest_state_mut(&mut self, mutate: impl FnOnce(&mut ManifestState)) {
        mutate(&mut self.manifest);
        self.debug_assert_invariants();
    }

    pub fn reset_manifest_lookup_state(&mut self) {
        self.with_manifest_state_mut(|manifest| {
            manifest.manifest_lookup = ManifestLookupState::default();
        });
    }

    pub fn needs_plan_bootstrap(&self, phase: Phase) -> bool {
        match phase {
            Phase::CleansePlan => !self.manifest.plan_bootstrap.cleanse_done,
            Phase::ModelPlan => !self.manifest.plan_bootstrap.model_done,
            _ => false,
        }
    }

    pub fn mark_plan_bootstrap_done(&mut self, phase: Phase) {
        self.with_manifest_state_mut(|manifest| match phase {
            Phase::CleansePlan => manifest.plan_bootstrap.cleanse_done = true,
            Phase::ModelPlan => manifest.plan_bootstrap.model_done = true,
            _ => {}
        });
    }

    pub fn reset_plan_bootstrap(&mut self, phase: Phase) {
        self.with_manifest_state_mut(|manifest| match phase {
            Phase::CleansePlan => manifest.plan_bootstrap.cleanse_done = false,
            Phase::ModelPlan => manifest.plan_bootstrap.model_done = false,
            _ => {}
        });
    }

    pub fn note_manifest_lookup_attempt(
        &mut self,
        path_kind: ManifestLookupPathKind,
        success: bool,
        failure_kind: Option<ManifestLookupFailureKind>,
    ) {
        let mut manifest = self.manifest.clone();
        if matches!(
            path_kind,
            ManifestLookupPathKind::Ambiguous | ManifestLookupPathKind::NonCanonical
        ) {
            manifest.manifest_lookup.noncanonical_attempt_count = manifest
                .manifest_lookup
                .noncanonical_attempt_count
                .saturating_add(1);
        }
        if success {
            if path_kind == ManifestLookupPathKind::CanonicalTarget {
                manifest.manifest_lookup.canonical_success_count = manifest
                    .manifest_lookup
                    .canonical_success_count
                    .saturating_add(1);
            }
            manifest.manifest_lookup.retry_suppressed =
                manifest.manifest_lookup.repeated_failure_count >= 2
                    && manifest.manifest_lookup.canonical_success_count == 0;
            self.manifest = manifest;
            self.debug_assert_invariants();
            return;
        }
        if let Some(kind) = failure_kind {
            let signature = format!("{}:{path_kind:?}", kind.as_str());
            let repeated = if manifest
                .manifest_lookup
                .failure_signature
                .as_deref()
                .map(|s| s == signature.as_str())
                .unwrap_or(false)
            {
                manifest
                    .manifest_lookup
                    .repeated_failure_count
                    .saturating_add(1)
            } else {
                1
            };
            manifest.manifest_lookup.failure_signature = Some(signature);
            manifest.manifest_lookup.repeated_failure_count = repeated;
        }
        manifest.manifest_lookup.retry_suppressed = manifest.manifest_lookup.repeated_failure_count
            >= 2
            && manifest.manifest_lookup.canonical_success_count == 0;
        self.manifest = manifest;
        self.debug_assert_invariants();
    }

    pub fn apply_validate_failure(
        &mut self,
        tier: ExecutionTier,
        failure_class: FailureKind,
        failure_signature: FailureSignature,
        repair_intent: RepairIntent,
        brief: Option<String>,
    ) {
        let backlog = repair_intent.backlog().to_vec();
        let (prev_count, prev_signature) = {
            let repair = self.repair_state();
            (
                repair.repair_backlog.len() as i64,
                repair.last_failure_signature.clone(),
            )
        };
        self.telemetry.last_validate = Some(LastValidateState {
            ts: Some(chrono::Utc::now().to_rfc3339()),
            ok: Some(false),
            compile_ok: Some(
                obs_like_bool(&self.telemetry.last_validate, |lv| lv.compile_ok).unwrap_or(false),
            ),
            run_ok: Some(
                obs_like_bool(&self.telemetry.last_validate, |lv| lv.run_ok).unwrap_or(false),
            ),
            brief: brief.clone(),
            failed_models: backlog
                .iter()
                .map(|t| FailedModelRef {
                    name: t.model_name.clone().unwrap_or_default(),
                    file: t
                        .path
                        .as_ref()
                        .map(|p| p.as_str().to_string())
                        .unwrap_or_default(),
                    ..Default::default()
                })
                .collect(),
            failure_class: Some(failure_class),
            ..LastValidateState::default()
        });
        let compile_ok = self
            .telemetry
            .last_validate
            .as_ref()
            .and_then(|v| v.compile_ok)
            .unwrap_or(false);
        let failed_target_count_delta = backlog.len() as i64 - prev_count;
        let failure_signature_changed = prev_signature != Some(failure_signature.clone());
        let progress_made = failure_signature_changed || failed_target_count_delta < 0;
        self.with_workflow_control_state_mut(|state| {
            state.phase.current_tier = tier;
            state.phase.mode = ExecutionMode::Mutate;

            state.repair.last_failure_signature = Some(failure_signature.clone());
            state.repair.repair_backlog = backlog;
            Self::enable_repair_mode(&mut state.repair, &repair_intent);
            state.repair.last_error_brief = brief;
            if progress_made {
                state.repair.stall_count = 0;
            } else {
                state.repair.stall_count = state.repair.stall_count.saturating_add(1);
            }
            state.repair.last_progress_delta = Some(ProgressDelta {
                target_hash_changed: false,
                failed_target_count_delta,
                failure_signature_changed,
                checklist_completed_delta: 0,
                progress_made,
            });

            state.publish.publish_approval = None;
            state.probe = ProbeState::default();
            state.probe.required = matches!(
                repair_intent,
                RepairIntent::SqlPatch { .. } | RepairIntent::SqlRewrite { .. }
            ) && compile_ok;
        });
    }

    pub fn note_patch_attempt(&mut self, ok: bool, mutated: bool) {
        self.with_workflow_control_state_mut(|state| {
            if let Some(core) = state.repair.core_mut() {
                core.attempt_count = core.attempt_count.saturating_add(1);
            }
            if ok && mutated {
                if let Some(core) = state.repair.core_mut() {
                    core.consecutive_noop_patches = 0;
                    core.ladder_step = RepairLadderStep::PatchTarget;
                }
                if let RepairModeState::SqlTarget(mode) = &mut state.repair.repair_mode {
                    mode.note_materialized();
                }
                state.repair.stall_count = 0;
                state.repair.last_progress_delta = Some(ProgressDelta {
                    target_hash_changed: true,
                    progress_made: true,
                    ..ProgressDelta::default()
                });
                state.probe.required = false;
                state.probe.repeated_signature_streak = 0;
                return;
            }
            if let Some(core) = state.repair.core_mut() {
                core.consecutive_noop_patches = core.consecutive_noop_patches.saturating_add(1);
                core.ladder_step = match core.ladder_step {
                    RepairLadderStep::PatchTarget => RepairLadderStep::ReplaceContents,
                    RepairLadderStep::ReplaceContents => RepairLadderStep::FsOp,
                    RepairLadderStep::FsOp => RepairLadderStep::Stop,
                    RepairLadderStep::Stop => RepairLadderStep::Stop,
                };
            }
            state.repair.stall_count = state.repair.stall_count.saturating_add(1);
            state.repair.last_progress_delta = Some(ProgressDelta {
                target_hash_changed: false,
                progress_made: false,
                ..ProgressDelta::default()
            });
        });
    }

    pub fn reset_probe_state_on_validate(&mut self, is_failure: bool) {
        self.with_probe_state_mut(|probe| {
            *probe = ProbeState::default();
            probe.required = is_failure;
        });
    }

    pub fn note_probe_attempt(
        &mut self,
        sql: &str,
        ok: bool,
        signature: ProbeSignature,
    ) -> ProbeOutcomeKind {
        let meaningful_sql = is_meaningful_probe_sql(sql);
        let mut outcome = ProbeOutcomeKind::Failed;
        self.with_probe_state_mut(|probe| {
            probe.attempts_total = probe.attempts_total.saturating_add(1);
            outcome = if !ok {
                probe.failed_attempts = probe.failed_attempts.saturating_add(1);
                probe.repeated_signature_streak = probe.repeated_signature_streak.saturating_add(1);
                ProbeOutcomeKind::Failed
            } else if !meaningful_sql {
                probe.non_meaningful_attempts = probe.non_meaningful_attempts.saturating_add(1);
                probe.repeated_signature_streak = probe.repeated_signature_streak.saturating_add(1);
                ProbeOutcomeKind::NonMeaningful
            } else if probe.last_signature.as_ref() == Some(&signature) {
                probe.meaningful_attempts = probe.meaningful_attempts.saturating_add(1);
                probe.repeated_signature_streak = probe.repeated_signature_streak.saturating_add(1);
                ProbeOutcomeKind::MeaningfulSameSignal
            } else {
                probe.meaningful_attempts = probe.meaningful_attempts.saturating_add(1);
                probe.repeated_signature_streak = 0;
                ProbeOutcomeKind::MeaningfulNewSignal
            };
            probe.last_signature = Some(signature.clone());
        });
        outcome
    }

    pub fn probe_requirement_status(&self) -> ProbeRequirementStatus {
        let probe = self.probe_state_snapshot();
        if !self.last_validate_failed() || !probe.required {
            return ProbeRequirementStatus::NotRequired;
        }
        if probe.meaningful_attempts == 0 {
            return ProbeRequirementStatus::Required;
        }
        if probe.repeated_signature_streak >= 3
            || probe.non_meaningful_attempts >= 3
            || probe.failed_attempts >= 3
        {
            return ProbeRequirementStatus::ExhaustedRequireMutation;
        }
        ProbeRequirementStatus::Allowed
    }

    pub fn bump_subjective_retry(&mut self, kind: SubjectiveRetryKind, cap: usize) -> usize {
        let entry = self.subjective_retries.entry(kind).or_insert(0);
        *entry = (*entry).saturating_add(1).min(cap.max(1));
        *entry
    }

    pub fn clear_subjective_retry_kind(&mut self, kind: SubjectiveRetryKind) {
        self.subjective_retries.remove(&kind);
    }

    pub fn clear_subjective_retries_matching(&mut self, f: impl Fn(&SubjectiveRetryKind) -> bool) {
        self.subjective_retries.retain(|k, _| !f(k));
    }

    pub fn set_pending_patch_impl_intent(&mut self, phase: Phase) {
        let epoch = self.repair.mutation_epoch;
        self.with_repair_state_mut(|repair| {
            repair.pending_patch_impl = Some(PatchImplIntent {
                phase,
                entry_mutation_epoch: epoch,
            });
            repair.stall_count = 0;
        });
    }

    pub fn set_pending_plan_revision(
        &mut self,
        violations: Vec<PlanViolation>,
        strategy: PlanRevisionStrategy,
    ) {
        self.with_phase_state_mut(|phase| {
            phase.pending_plan_revision = Some(PlanRevisionIntent {
                violations,
                strategy,
            });
        });
    }

    pub fn take_pending_plan_revision(&mut self) -> Option<PlanRevisionIntent> {
        let rev = self.phase.pending_plan_revision.take();
        if rev.is_some() {
            self.debug_assert_invariants();
        }
        rev
    }

    pub fn clear_pending_patch_impl(&mut self) {
        self.with_repair_state_mut(|repair| {
            repair.pending_patch_impl = None;
        });
    }

    pub fn set_publish_approval(&mut self, decision: PublishApprovalDecision) {
        self.with_publish_state_mut(|publish| {
            publish.publish_approval = Some(PublishApprovalState {
                decision,
                ts: chrono::Utc::now().to_rfc3339(),
            });
        });
    }

    pub fn clear_publish_approval(&mut self) {
        self.with_publish_state_mut(|publish| {
            publish.publish_approval = None;
        });
    }

    pub fn is_publish_approved(&self) -> bool {
        self.publish
            .publish_approval
            .as_ref()
            .map(|s| s.decision == PublishApprovalDecision::Approved)
            .unwrap_or(false)
    }

    pub fn bump_publish_retry(&mut self, kind: PublishRetryKind, cap: usize) -> usize {
        let capped = cap.max(1);
        if let Some(existing) = self
            .publish
            .publish_retries
            .iter_mut()
            .find(|r| r.kind == kind)
        {
            existing.count = existing.count.saturating_add(1).min(capped);
            let count = existing.count;
            self.debug_assert_invariants();
            return count;
        }
        self.publish
            .publish_retries
            .push(PublishRetryState { kind, count: 1 });
        self.debug_assert_invariants();
        1
    }

    pub fn reset_publish_retry(&mut self, kind: PublishRetryKind) {
        self.publish.publish_retries.retain(|r| r.kind != kind);
        self.debug_assert_invariants();
    }

    pub fn reset_publish_retries(&mut self) {
        self.with_publish_state_mut(|publish| {
            publish.publish_retries.clear();
        });
    }

    pub fn enter_validate_mode(&mut self, tier: ExecutionTier) {
        self.with_phase_state_mut(|phase| {
            phase.current_tier = tier;
            phase.mode = ExecutionMode::Validate;
        });
    }

    pub fn mark_failed(&mut self, brief: impl Into<String>) {
        let brief = brief.into();
        self.with_phase_state_mut(|phase| {
            phase.mode = ExecutionMode::Failed;
        });
        self.with_repair_state_mut(|repair| {
            repair.last_error_brief = Some(brief);
        });
    }

    pub async fn load(
        control: &ControlStateStore,
        thread_id: &str,
    ) -> Result<Option<Self>, String> {
        crate::state_manager::load_execution_state(control, thread_id)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn load_strict(
        control: &ControlStateStore,
        thread_id: &str,
    ) -> Result<Option<Self>, String> {
        crate::state_manager::load_execution_state_strict(control, thread_id)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn save(&self, control: &ControlStateStore, thread_id: &str) -> Result<(), String> {
        crate::state_manager::replace_execution_state(control, thread_id, self.clone())
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn set_pending_publish_plan(&mut self, plan_sha256: String) {
        self.with_publish_state_mut(|publish| {
            publish.publish_plan.pending_plan_sha256 = Some(plan_sha256);
            publish.publish_plan.pending_set_ts = Some(chrono::Utc::now().to_rfc3339());
        });
    }

    pub fn mark_publish_complete(&mut self, plan_sha256: String) {
        self.with_publish_state_mut(|publish| {
            publish.publish_plan.last_published_plan_sha256 = Some(plan_sha256);
            publish.publish_plan.published_ts = Some(chrono::Utc::now().to_rfc3339());
            publish.publish_plan.pending_plan_sha256 = None;
            publish.publish_plan.pending_set_ts = None;
        });
    }

    pub fn set_artifact_focus(
        &mut self,
        kind: Option<String>,
        name: Option<String>,
        dataset_id: Option<String>,
        exists: bool,
    ) {
        self.telemetry.artifact_focus = Some(ArtifactFocusState {
            kind,
            name,
            dataset_id,
            exists,
            ts: Some(chrono::Utc::now().to_rfc3339()),
        });
    }

    pub fn set_last_mutation_summary(
        &mut self,
        op: MutationOp,
        affected_paths: Vec<String>,
        select_terms: Vec<String>,
    ) {
        self.repair.mutation_epoch = self.repair.mutation_epoch.saturating_add(1);
        self.telemetry.last_mutation_summary = Some(LastMutationSummary {
            op,
            affected_paths,
            select_terms,
            ts: Some(chrono::Utc::now().to_rfc3339()),
        });
        self.debug_assert_invariants();
    }

    /// Track authoring step-boundary progress for stall detection.
    /// Resets stall_count on mutation progress; increments it otherwise.
    pub fn record_stepboundary_progress(&mut self, mutation_advanced: bool) {
        if mutation_advanced {
            self.repair.stall_count = 0;
        } else {
            self.repair.stall_count = self.repair.stall_count.saturating_add(1);
        }
    }

    pub fn apply_event(&mut self, event: DataEngineerEvent) {
        match event {
            DataEngineerEvent::ValidatePassed { tier } => self.apply_validate_success(tier),
            DataEngineerEvent::ValidateFailed {
                tier,
                failure_class,
                failure_signature,
                repair_intent,
                brief,
                compile_ok,
                run_ok,
            } => {
                self.apply_validate_failure(
                    tier,
                    failure_class,
                    failure_signature,
                    repair_intent,
                    brief,
                );
                if let Some(last) = self.telemetry.last_validate.as_mut() {
                    if let Some(v) = compile_ok {
                        last.compile_ok = Some(v);
                    }
                    if let Some(v) = run_ok {
                        last.run_ok = Some(v);
                    }
                }
            }
            DataEngineerEvent::BatchAuthoringFailed {
                tier,
                kind,
                failed_targets,
                brief,
            } => {
                if kind == FailureKind::InfraTransient {
                    self.repair.last_batch_infra_transient = true;
                    self.repair.last_error_brief = Some(brief);
                    self.debug_assert_invariants();
                    return;
                }
                let failure_class = kind;
                let backlog = repair_backlog_from_failed_models(failure_class, &failed_targets);
                let repair_intent = repair_intent_from_backlog(failure_class, backlog.clone());
                let sig = FailureSignature {
                    class: failure_class,
                    node_id: failed_targets
                        .first()
                        .map(|f| f.name.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    canonical_path: failed_targets
                        .first()
                        .and_then(|f| RepairTargetPath::parse(f.file.trim().to_string()).ok()),
                    error_code: Some(format!("batch_{:?}", kind).to_ascii_lowercase()),
                };
                self.apply_validate_failure(tier, failure_class, sig, repair_intent, Some(brief));
            }
            DataEngineerEvent::BatchAuthoringRecovered => {
                Self::disable_repair_mode(&mut self.repair);
                self.repair.last_error_brief = None;
                self.repair.pending_patch_impl = None;
                self.debug_assert_invariants();
            }
        }
        self.debug_assert_invariants();
    }

    pub fn validate_invariants(&self) -> Result<(), String> {
        let mut violations = Vec::new();
        self.collect_phase_coherence_violations(&mut violations);
        self.collect_repair_ladder_coherence_violations(&mut violations);
        self.collect_publish_coherence_violations(&mut violations);
        self.collect_probe_lifecycle_violations(&mut violations);
        if violations.is_empty() {
            return Ok(());
        }
        Err(format!(
            "execution_state invariant violation(s): {}",
            violations.join("; ")
        ))
    }

    fn collect_phase_coherence_violations(&self, violations: &mut Vec<String>) {
        let phase = self.phase_state();
        if phase.phase_reason_code.is_some() && phase.current_phase.is_none() {
            violations.push("phase_reason_code set while current_phase is none".to_string());
        }
        if phase.phase_reason_detail.is_some() && phase.current_phase.is_none() {
            violations.push("phase_reason_detail set while current_phase is none".to_string());
        }
    }

    fn collect_repair_ladder_coherence_violations(&self, violations: &mut Vec<String>) {
        let repair = self.repair_state();
        if let RepairModeState::SqlTarget(mode) = &repair.repair_mode {
            if mode.core.ladder_step == RepairLadderStep::Stop && mode.core.attempt_count < 3 {
                violations.push("repair ladder reached stop before three attempts".to_string());
            }
            if mode.materialization == RepairTargetMaterialization::Missing
                && mode.core.ladder_step == RepairLadderStep::PatchTarget
            {
                violations.push(
                    "sql target repair mode cannot enter patch_target when target content is missing"
                        .to_string(),
                );
            }
            if mode.target_path.as_str().trim().is_empty() {
                violations
                    .push("sql target repair mode requires non-empty target_path".to_string());
            }
            if !is_sql_model_path(mode.target_path.as_str()) {
                violations.push(
                    "sql target repair mode requires a .sql target_path under models/".to_string(),
                );
            }
        }
    }

    fn collect_publish_coherence_violations(&self, violations: &mut Vec<String>) {
        let publish = self.publish_state();
        if let Some(approval) = publish.publish_approval.as_ref() {
            if approval.ts.trim().is_empty() {
                violations.push("publish_approval timestamp must be non-empty".to_string());
            }
        }
        let mut kinds = HashSet::new();
        for retry in &publish.publish_retries {
            if retry.count == 0 {
                violations.push(format!("publish retry {:?} has zero count", retry.kind));
            }
            if !kinds.insert(retry.kind) {
                violations.push(format!(
                    "duplicate publish retry entry for {:?}",
                    retry.kind
                ));
            }
        }
    }

    fn collect_probe_lifecycle_violations(&self, violations: &mut Vec<String>) {
        let probe = self.probe_state_snapshot();
        if probe.required && !self.last_validate_failed() {
            violations.push(
                "probe.required can only be true while last_validate.ok is false".to_string(),
            );
        }
        let classified_attempts = probe
            .meaningful_attempts
            .saturating_add(probe.non_meaningful_attempts)
            .saturating_add(probe.failed_attempts);
        if classified_attempts > probe.attempts_total {
            violations.push("probe attempt counters exceed attempts_total".to_string());
        }
        if probe.repeated_signature_streak > probe.attempts_total {
            violations.push("probe repeated_signature_streak exceeds attempts_total".to_string());
        }
    }

    fn debug_assert_invariants(&self) {
        debug_assert!(
            self.validate_invariants().is_ok(),
            "invalid execution state: {:?}",
            self.validate_invariants()
        );
    }
}

pub fn normalize_manifest_path(path: &str) -> String {
    path.trim().trim_matches('/').replace('\\', "/")
}

pub fn classify_manifest_lookup_path(path: &str) -> Option<ManifestLookupPathKind> {
    let norm = normalize_manifest_path(path);
    if norm.is_empty() {
        return None;
    }
    if norm == "target/manifest.json" {
        return Some(ManifestLookupPathKind::CanonicalTarget);
    }
    if norm.ends_with("target/manifest.json") {
        return Some(ManifestLookupPathKind::NonCanonical);
    }
    if norm.ends_with("manifest.json") {
        return Some(ManifestLookupPathKind::Ambiguous);
    }
    None
}

pub fn classify_manifest_lookup_failure(errors: &[String]) -> Option<ManifestLookupFailureKind> {
    let joined = errors.join("\n").to_ascii_lowercase();
    if joined.contains("nosuchkey")
        || joined.contains("not found or failed to fetch")
        || joined.contains("the specified key does not exist")
    {
        return Some(ManifestLookupFailureKind::NoSuchKey);
    }
    if joined.contains("pointer not found") {
        return Some(ManifestLookupFailureKind::PointerNotFound);
    }
    None
}

fn obs_like_bool(
    from: &Option<LastValidateState>,
    pick: impl FnOnce(&LastValidateState) -> Option<bool>,
) -> Option<bool> {
    from.as_ref().and_then(pick)
}

fn sibling_sql_path(schema_path: &str) -> Option<String> {
    let s = schema_path.trim();
    if let Some(base) = s.strip_suffix(".yml") {
        Some(format!("{}.sql", base))
    } else if let Some(base) = s.strip_suffix(".yaml") {
        Some(format!("{}.sql", base))
    } else {
        None
    }
}

pub fn repair_backlog_from_failed_models(
    failure_class: FailureKind,
    failing_models: &[FailedModelRef],
) -> Vec<RepairTarget> {
    let mut out: Vec<RepairTarget> = failing_models
        .iter()
        .map(|fm| RepairTarget {
            model_name: Some(fm.name.trim().to_string()).filter(|s| !s.is_empty()),
            path: if fm.file.trim().is_empty() || fm.file.trim() == "(unknown file)" {
                None
            } else {
                RepairTargetPath::parse(fm.file.trim().to_string()).ok()
            },
            error_class: None,
            materialization: fm.materialization,
        })
        .filter(|target| match failure_class {
            FailureKind::Schema => target
                .path
                .as_ref()
                .map(|p| matches!(p, RepairTargetPath::SchemaDoc(_)))
                .unwrap_or(false),
            FailureKind::SqlRuntime => target
                .path
                .as_ref()
                .map(|p| {
                    matches!(
                        p,
                        RepairTargetPath::SqlModel(_) | RepairTargetPath::SchemaDoc(_)
                    )
                })
                .unwrap_or(false),
            FailureKind::WarehouseConfig
            | FailureKind::InfraTransient
            | FailureKind::MissingSource
            | FailureKind::NoFailure
            | FailureKind::Unknown => false,
        })
        .collect();

    if failure_class == FailureKind::SqlRuntime {
        let siblings: Vec<RepairTarget> = out
            .iter()
            .filter_map(|t| match &t.path {
                Some(RepairTargetPath::SchemaDoc(doc)) => {
                    let sql = sibling_sql_path(doc.as_str())?;
                    let sql_path = SqlModelPath::parse(sql).ok()?;
                    Some(RepairTarget {
                        model_name: t.model_name.clone(),
                        path: Some(RepairTargetPath::SqlModel(sql_path)),
                        error_class: None,
                        materialization: RepairTargetMaterialization::Unknown,
                    })
                }
                _ => None,
            })
            .collect();
        out.extend(siblings);
    }

    out.sort_by(|a, b| {
        a.path
            .as_ref()
            .map(|p| p.as_str())
            .cmp(&b.path.as_ref().map(|p| p.as_str()))
            .then(a.model_name.cmp(&b.model_name))
    });
    out.dedup_by(|a, b| a.path == b.path && a.model_name == b.model_name);
    out
}

fn is_sql_model_path(path: &str) -> bool {
    let normalized = path.trim().replace('\\', "/");
    normalized.starts_with("models/")
        && normalized.ends_with(".sql")
        && !normalized.contains(".yml/")
        && !normalized.contains(".yaml/")
}

fn is_schema_doc_path(path: &str) -> bool {
    let normalized = path.trim().replace('\\', "/");
    normalized.starts_with("models/")
        && (normalized.ends_with(".yml") || normalized.ends_with(".yaml"))
}

pub fn repair_intent_from_backlog(
    failure_class: FailureKind,
    backlog: Vec<RepairTarget>,
) -> RepairIntent {
    match failure_class {
        FailureKind::Schema => RepairIntent::Schema { backlog },
        FailureKind::SqlRuntime => {
            let sql_targets: Vec<(SqlModelPath, RepairTargetMaterialization)> = backlog
                .iter()
                .filter_map(|item| match &item.path {
                    Some(RepairTargetPath::SqlModel(path)) => {
                        Some((path.clone(), item.materialization))
                    }
                    _ => None,
                })
                .collect();
            if sql_targets.len() > 1 {
                RepairIntent::NoRepair {
                    reason: NoRepairReason::MultiTargetSqlFailure,
                    backlog,
                }
            } else if let Some((target, materialization)) = sql_targets.into_iter().next() {
                match materialization {
                    RepairTargetMaterialization::Existing => {
                        RepairIntent::SqlPatch { target, backlog }
                    }
                    RepairTargetMaterialization::Missing => {
                        RepairIntent::SqlRewrite { target, backlog }
                    }
                    RepairTargetMaterialization::Unknown => RepairIntent::NoRepair {
                        reason: NoRepairReason::MissingTargetMaterialization,
                        backlog,
                    },
                }
            } else if backlog
                .iter()
                .any(|item| matches!(item.path, Some(RepairTargetPath::SchemaDoc(_))))
            {
                RepairIntent::Schema { backlog }
            } else {
                RepairIntent::NoRepair {
                    reason: NoRepairReason::NoRepairableTarget,
                    backlog,
                }
            }
        }
        FailureKind::WarehouseConfig
        | FailureKind::InfraTransient
        | FailureKind::MissingSource
        | FailureKind::NoFailure
        | FailureKind::Unknown => RepairIntent::NoRepair {
            reason: NoRepairReason::FailureClassNotRepairable,
            backlog,
        },
    }
}

#[cfg(test)]
pub fn failed_model_refs_from_values(values: &[Value]) -> Vec<FailedModelRef> {
    values
        .iter()
        .map(|v| FailedModelRef {
            name: v
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .trim()
                .to_string(),
            file: v
                .get("file")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .trim()
                .to_string(),
            materialization: serde_json::from_value(
                v.get("materialization")
                    .cloned()
                    .unwrap_or_else(|| Value::String("unknown".to_string())),
            )
            .unwrap_or(RepairTargetMaterialization::Unknown),
            ..Default::default()
        })
        .collect()
}

pub fn gate_authoring_progress(state: &ExecutionState, phase: Phase) -> Result<(), String> {
    let last_validate_failed =
        state.telemetry.last_validate.as_ref().and_then(|lv| lv.ok) == Some(false);
    let mutation_progress = state
        .repair
        .last_progress_delta
        .as_ref()
        .map(|d| d.progress_made || d.target_hash_changed)
        .unwrap_or(false);
    let mutation_epoch_advanced = state
        .repair
        .core()
        .and_then(|core| core.repair_started_mutation_epoch)
        .map(|start| state.repair.mutation_epoch > start)
        .unwrap_or(false);
    if last_validate_failed && !(mutation_progress || mutation_epoch_advanced) {
        return Err(
            "progress_gate_blocked: validation previously failed and no successful mutation has been recorded since that failure"
                .to_string(),
        );
    }
    match state.probe_requirement_status() {
        ProbeRequirementStatus::Required => {
            return Err(
                "progress_gate_blocked: runtime validation previously failed after compile and a meaningful data probe is still required"
                    .to_string(),
            );
        }
        ProbeRequirementStatus::ExhaustedRequireMutation => {
            return Err(
                "progress_gate_blocked: probe loop exhausted (repeated/no-new-signal probes); apply a mutating fix before validating"
                    .to_string(),
            );
        }
        ProbeRequirementStatus::NotRequired | ProbeRequirementStatus::Allowed => {}
    }
    // Keep existing unresolved mutation-failure behavior, but as a deterministic progress gate.
    match phase {
        Phase::CleanseAuthor | Phase::ModelAuthor => {}
        _ => return Ok(()),
    }
    Ok(())
}

pub fn snapshot_authoring_stepboundary_progress(
    pre_mutation_epoch: u64,
    post_mutation_epoch: u64,
    post_stall_count: usize,
) -> AuthoringProgressSnapshot {
    if post_mutation_epoch <= pre_mutation_epoch
        && post_stall_count >= DEFAULT_MAX_STALL_COUNT.max(1)
    {
        return AuthoringProgressSnapshot {
            progress_made: false,
            reason: Some(AuthoringNoProgressReason::NoMutationProgress),
        };
    }
    AuthoringProgressSnapshot {
        progress_made: true,
        reason: None,
    }
}

pub fn is_meaningful_probe_sql(sql: &str) -> bool {
    let s = sql.trim().trim_end_matches(';').trim().to_lowercase();
    if s.is_empty() {
        return false;
    }
    let toks: Vec<&str> = s.split_whitespace().collect();
    if toks == ["select", "1"] {
        return false;
    }
    if toks.len() == 4 && toks[0] == "select" && toks[1] == "1" && toks[2] == "as" {
        return false;
    }
    toks.iter().any(|t| *t == "from")
}

pub fn gate_publish_progress(state: &ExecutionState, phase: Phase) -> Result<(), String> {
    match phase {
        Phase::PublishAwaitApproval | Phase::Publish => {}
        _ => return Ok(()),
    }
    if state.is_publish_approved() {
        return Ok(());
    }
    Err("publish_gate_blocked: publish requires explicit persisted approval state".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sql_model_path(path: &str) -> SqlModelPath {
        SqlModelPath::parse(path.to_string()).expect("valid sql model path")
    }

    fn repair_target_path(path: &str) -> RepairTargetPath {
        RepairTargetPath::parse(path.to_string()).expect("valid repair target path")
    }

    use react_core::keyspace::DefaultKeyspace;
    use react_core::scope::RequestScope;
    use react_core::session::ControlStateStore;
    use react_module_storage_memory::InMemoryStorageAdapter;
    use std::sync::Arc;

    #[test]
    fn state_has_compile_time_defaults() {
        let st = ExecutionState::new();
        assert_eq!(st.phase.current_tier, ExecutionTier::Unknown);
        assert_eq!(st.phase.mode, ExecutionMode::Discover);
    }

    #[test]
    fn authoring_stepboundary_snapshot_flags_no_mutation_progress() {
        let snapshot = snapshot_authoring_stepboundary_progress(4, 4, 3);
        assert!(!snapshot.progress_made);
        assert_eq!(
            snapshot.reason,
            Some(AuthoringNoProgressReason::NoMutationProgress)
        );
    }

    #[test]
    fn authoring_stepboundary_snapshot_accepts_mutation_progress() {
        let snapshot = snapshot_authoring_stepboundary_progress(4, 5, 0);
        assert!(snapshot.progress_made);
        assert_eq!(snapshot.reason, None);
    }

    #[test]
    fn authoring_stepboundary_snapshot_allows_single_non_mutating_turn_before_budget() {
        let snapshot = snapshot_authoring_stepboundary_progress(5, 5, 1);
        assert!(snapshot.progress_made);
        assert_eq!(snapshot.reason, None);
    }

    #[test]
    fn record_stepboundary_progress_increments_on_no_mutation() {
        let mut st = ExecutionState::new();
        assert_eq!(st.repair.stall_count, 0);
        st.record_stepboundary_progress(false);
        assert_eq!(st.repair.stall_count, 1);
        st.record_stepboundary_progress(false);
        assert_eq!(st.repair.stall_count, 2);
        st.record_stepboundary_progress(true);
        assert_eq!(st.repair.stall_count, 0);
    }

    #[test]
    fn repair_prompt_context_includes_recent_failed_file_ops() {
        let ctx = RepairPromptContext {
            recent_failed_file_ops: vec![RecentFailedFileOp {
                op: "patch".to_string(),
                path: "models/staging/stg_orders.yml".to_string(),
                error_brief: "invalid YAML: duplicate entry with key \"version\"".to_string(),
            }],
            ..Default::default()
        };
        let rendered = ctx.format_error_context();
        assert!(ctx.has_context());
        assert!(rendered.contains("Recent failed file mutations"));
        assert!(rendered.contains("stg_orders.yml"));
        assert!(rendered.contains("duplicate entry with key"));
    }

    #[test]
    fn validate_success_resets_repair_and_retry_state() {
        let mut st = ExecutionState::new();
        st.repair.repair_mode = RepairModeState::SqlTarget(SqlTargetRepairMode {
            target_path: sql_model_path("models/marts/fct_orders.sql"),
            materialization: RepairTargetMaterialization::Existing,
            core: RepairModeCore {
                ladder_step: RepairLadderStep::PatchTarget,
                attempt_count: 0,
                repair_started_mutation_epoch: None,
                consecutive_noop_patches: 0,
            },
        });
        st.subjective_retries
            .insert(SubjectiveRetryKind::PlanSemanticInvalid, 3);
        st.subjective_retries
            .insert(SubjectiveRetryKind::ValidatePrecheckFailed, 2);
        st.apply_validate_success(ExecutionTier::Model);
        assert_eq!(st.phase.current_tier, ExecutionTier::Model);
        assert_eq!(st.phase.mode, ExecutionMode::Done);
        assert!(!st.hard_mutation_repair_mode());
        assert_eq!(st.repair_type(), RepairType::Unknown);
        assert_eq!(
            st.subjective_retries
                .get(&SubjectiveRetryKind::PlanSemanticInvalid),
            Some(&3)
        );
        assert_eq!(
            st.subjective_retries
                .get(&SubjectiveRetryKind::ValidatePrecheckFailed),
            None
        );
    }

    #[test]
    fn validate_failure_sets_typed_repair_type() {
        let mut st = ExecutionState::new();
        let sig = FailureSignature {
            class: FailureKind::Schema,
            ..FailureSignature::default()
        };
        st.apply_validate_failure(
            ExecutionTier::Cleanse,
            FailureKind::Schema,
            sig,
            RepairIntent::Schema {
                backlog: Vec::new(),
            },
            Some("schema fail".to_string()),
        );
        assert_eq!(st.repair_type(), RepairType::Schema);

        let sig2 = FailureSignature {
            class: FailureKind::SqlRuntime,
            ..FailureSignature::default()
        };
        st.apply_validate_failure(
            ExecutionTier::Cleanse,
            FailureKind::SqlRuntime,
            sig2,
            RepairIntent::SqlPatch {
                target: sql_model_path("models/staging/stg_orders.sql"),
                backlog: vec![RepairTarget {
                    model_name: Some("model.pkg.stg_orders".to_string()),
                    path: Some(repair_target_path("models/staging/stg_orders.sql")),
                    error_class: Some(FailureKind::SqlRuntime),
                    materialization: RepairTargetMaterialization::Existing,
                }],
            },
            Some("sql fail".to_string()),
        );
        assert_eq!(st.repair_type(), RepairType::SqlTarget);
    }

    #[test]
    fn validate_failure_sql_runtime_with_schema_target_routes_to_schema_repair() {
        let mut st = ExecutionState::new();
        let sig = FailureSignature {
            class: FailureKind::SqlRuntime,
            ..FailureSignature::default()
        };
        st.apply_validate_failure(
            ExecutionTier::Cleanse,
            FailureKind::SqlRuntime,
            sig,
            RepairIntent::Schema {
                backlog: vec![RepairTarget {
                    model_name: Some("test.not_null_stg_orders_order_id".to_string()),
                    path: Some(repair_target_path("models/staging/stg_orders.yml")),
                    error_class: Some(FailureKind::SqlRuntime),
                    materialization: RepairTargetMaterialization::Existing,
                }],
            },
            Some("dbt test fail".to_string()),
        );
        assert_eq!(st.repair_type(), RepairType::Schema);
        assert!(st.single_target_repair_path().is_none());
    }

    #[test]
    fn sql_model_path_rejects_dbt_compiled_test_paths() {
        assert!(SqlModelPath::parse("models/staging/stg_orders.sql").is_ok());
        assert!(SqlModelPath::parse(
            "models/staging/stg_orders.yml/not_null_stg_orders_order_id.sql"
        )
        .is_err());
        assert!(SqlModelPath::parse(
            "models/staging/stg_orders.yaml/not_null_stg_orders_order_id.sql"
        )
        .is_err());
        let target = RepairTargetPath::parse(
            "models/staging/stg_orders.yml/not_null_stg_orders_order_id.sql".to_string(),
        );
        assert!(
            target.is_err(),
            "dbt compiled test path must not parse as RepairTargetPath"
        );
    }

    #[test]
    fn repair_backlog_filters_targets_by_failure_class() {
        let failing = vec![
            FailedModelRef {
                name: "model.pkg.stg_orders".to_string(),
                file: "models/staging/stg_orders.sql".to_string(),
                materialization: RepairTargetMaterialization::Existing,
                ..Default::default()
            },
            FailedModelRef {
                name: "model.pkg.stg_orders".to_string(),
                file: "models/staging/stg_orders.yml".to_string(),
                materialization: RepairTargetMaterialization::Existing,
                ..Default::default()
            },
            FailedModelRef {
                name: "model.pkg.readme".to_string(),
                file: "README.md".to_string(),
                ..Default::default()
            },
        ];

        let schema_backlog = repair_backlog_from_failed_models(FailureKind::Schema, &failing);
        assert_eq!(schema_backlog.len(), 1);
        assert_eq!(
            schema_backlog[0].path.as_ref().map(|p| p.as_str()),
            Some("models/staging/stg_orders.yml")
        );

        let sql_backlog = repair_backlog_from_failed_models(FailureKind::SqlRuntime, &failing);
        assert_eq!(sql_backlog.len(), 2);
        assert!(sql_backlog
            .iter()
            .any(|entry| entry.path.as_ref().map(|p| p.as_str())
                == Some("models/staging/stg_orders.sql")));
        assert!(sql_backlog
            .iter()
            .any(|entry| entry.path.as_ref().map(|p| p.as_str())
                == Some("models/staging/stg_orders.yml")));

        let unknown_backlog = repair_backlog_from_failed_models(FailureKind::Unknown, &failing);
        assert!(unknown_backlog.is_empty());
    }

    #[test]
    fn repair_backlog_derives_sibling_sql_for_schema_only_sql_runtime() {
        let failing = vec![FailedModelRef {
            name: "not_null_stg_orders_line_id".to_string(),
            file: "models/staging/stg_orders.yml".to_string(),
            materialization: RepairTargetMaterialization::Existing,
            ..Default::default()
        }];
        let backlog = repair_backlog_from_failed_models(FailureKind::SqlRuntime, &failing);
        assert!(
            backlog
                .iter()
                .any(|t| t.path.as_ref().map(|p| p.as_str())
                    == Some("models/staging/stg_orders.sql")),
            "SqlRuntime with only .yml target should derive sibling .sql: {backlog:?}"
        );
        assert!(
            backlog
                .iter()
                .any(|t| t.path.as_ref().map(|p| p.as_str())
                    == Some("models/staging/stg_orders.yml")),
            "original .yml target should be preserved"
        );
    }

    #[test]
    fn repair_intent_uses_batch_mode_for_multi_target_sql_runtime() {
        let backlog = vec![
            RepairTarget {
                model_name: Some("stg_orders".to_string()),
                path: Some(repair_target_path("models/staging/stg_orders.sql")),
                error_class: Some(FailureKind::SqlRuntime),
                materialization: RepairTargetMaterialization::Existing,
            },
            RepairTarget {
                model_name: Some("stg_order_items".to_string()),
                path: Some(repair_target_path("models/staging/stg_order_items.sql")),
                error_class: Some(FailureKind::SqlRuntime),
                materialization: RepairTargetMaterialization::Existing,
            },
        ];
        let intent = repair_intent_from_backlog(FailureKind::SqlRuntime, backlog);
        assert!(matches!(
            intent,
            RepairIntent::NoRepair {
                reason: NoRepairReason::MultiTargetSqlFailure,
                ..
            }
        ));
    }

    #[test]
    fn repair_intent_requires_materialization_evidence_for_sql_targets() {
        let backlog = vec![RepairTarget {
            model_name: Some("model.pkg.stg_orders".to_string()),
            path: Some(repair_target_path("models/staging/stg_orders.sql")),
            error_class: Some(FailureKind::SqlRuntime),
            materialization: RepairTargetMaterialization::Unknown,
        }];
        let intent = repair_intent_from_backlog(FailureKind::SqlRuntime, backlog);
        assert!(matches!(
            intent,
            RepairIntent::NoRepair {
                reason: NoRepairReason::MissingTargetMaterialization,
                ..
            }
        ));
    }

    #[test]
    fn subjective_retry_per_kind_and_bounded() {
        let mut st = ExecutionState::new();
        assert_eq!(
            st.bump_subjective_retry(SubjectiveRetryKind::PlanSemanticInvalid, 3),
            1
        );
        assert_eq!(
            st.bump_subjective_retry(SubjectiveRetryKind::PlanSemanticInvalid, 3),
            2
        );
        assert_eq!(
            st.bump_subjective_retry(SubjectiveRetryKind::PlanSemanticInvalid, 3),
            3
        );
        assert_eq!(
            st.bump_subjective_retry(SubjectiveRetryKind::PlanSemanticInvalid, 3),
            3
        );

        assert_eq!(
            st.bump_subjective_retry(SubjectiveRetryKind::PlanGroundingEmptyAfterPrune, 3),
            1
        );
        assert_eq!(
            st.subjective_retries
                .get(&SubjectiveRetryKind::PlanSemanticInvalid),
            Some(&3)
        );

        st.clear_subjective_retry_kind(SubjectiveRetryKind::PlanSemanticInvalid);
        assert_eq!(
            st.subjective_retries
                .get(&SubjectiveRetryKind::PlanSemanticInvalid),
            None
        );
        assert_eq!(
            st.subjective_retries
                .get(&SubjectiveRetryKind::PlanGroundingEmptyAfterPrune),
            Some(&1)
        );

        st.clear_subjective_retries_matching(|k| {
            matches!(k, SubjectiveRetryKind::PlanGroundingEmptyAfterPrune)
        });
        assert!(st.subjective_retries.is_empty());
    }

    #[test]
    fn gate_authoring_progress_uses_execution_state() {
        let mut st = ExecutionState::new();
        st.telemetry.last_validate = Some(LastValidateState {
            ok: Some(false),
            compile_ok: Some(true),
            run_ok: Some(false),
            ..LastValidateState::default()
        });
        st.telemetry.probe.required = true;
        assert!(gate_authoring_progress(&st, Phase::ModelAuthor).is_err());

        st.repair.repair_mode = RepairModeState::SqlTarget(SqlTargetRepairMode {
            target_path: sql_model_path("models/marts/fct_orders.sql"),
            materialization: RepairTargetMaterialization::Existing,
            core: RepairModeCore {
                attempt_count: 1,
                ladder_step: RepairLadderStep::PatchTarget,
                repair_started_mutation_epoch: None,
                consecutive_noop_patches: 0,
            },
        });
        st.repair.last_progress_delta = Some(ProgressDelta {
            target_hash_changed: true,
            progress_made: true,
            ..ProgressDelta::default()
        });
        st.telemetry.last_validate = Some(LastValidateState {
            compile_ok: Some(true),
            run_ok: Some(true),
            ..LastValidateState::default()
        });
        st.telemetry.probe.required = false;
        assert!(gate_authoring_progress(&st, Phase::ModelAuthor).is_ok());
    }

    #[test]
    fn gate_authoring_progress_rejects_stale_mutation_receipts() {
        let mut st = ExecutionState::new();
        st.telemetry.last_validate = Some(LastValidateState {
            ok: Some(false),
            compile_ok: Some(true),
            run_ok: Some(false),
            ..LastValidateState::default()
        });
        st.repair.repair_mode = RepairModeState::SqlTarget(SqlTargetRepairMode {
            target_path: sql_model_path("models/marts/fct_orders.sql"),
            materialization: RepairTargetMaterialization::Existing,
            core: RepairModeCore {
                attempt_count: 1,
                ladder_step: RepairLadderStep::PatchTarget,
                repair_started_mutation_epoch: Some(4),
                consecutive_noop_patches: 0,
            },
        });
        st.repair.mutation_epoch = 4;
        st.telemetry.last_mutation_summary = Some(LastMutationSummary {
            op: MutationOp::Patch,
            affected_paths: vec!["models/marts/fct_orders.sql".to_string()],
            select_terms: vec![],
            ts: Some(chrono::Utc::now().to_rfc3339()),
        });
        assert!(gate_authoring_progress(&st, Phase::ModelAuthor).is_err());
    }

    #[test]
    fn gate_authoring_progress_accepts_mutation_epoch_advance_after_failure() {
        let mut st = ExecutionState::new();
        st.telemetry.last_validate = Some(LastValidateState {
            ok: Some(false),
            compile_ok: Some(true),
            run_ok: Some(false),
            ..LastValidateState::default()
        });
        st.repair.repair_mode = RepairModeState::SqlTarget(SqlTargetRepairMode {
            target_path: sql_model_path("models/marts/fct_orders.sql"),
            materialization: RepairTargetMaterialization::Existing,
            core: RepairModeCore {
                attempt_count: 1,
                ladder_step: RepairLadderStep::PatchTarget,
                repair_started_mutation_epoch: Some(4),
                consecutive_noop_patches: 0,
            },
        });
        st.repair.mutation_epoch = 5;
        st.telemetry.last_mutation_summary = Some(LastMutationSummary {
            op: MutationOp::Patch,
            affected_paths: vec!["models/marts/fct_orders.sql".to_string()],
            select_terms: vec![],
            ts: Some(chrono::Utc::now().to_rfc3339()),
        });
        assert!(gate_authoring_progress(&st, Phase::ModelAuthor).is_ok());
    }

    #[test]
    fn probe_status_allows_multiple_meaningful_probes_and_exhausts_on_repeats() {
        let mut st = ExecutionState::new();
        st.telemetry.last_validate = Some(LastValidateState {
            ok: Some(false),
            ..LastValidateState::default()
        });
        st.telemetry.probe.required = true;

        let sig1 = ProbeSignature::from_run_sql(
            "select * from x limit 10",
            &serde_json::json!({"ok":true}),
        );
        let out1 = st.note_probe_attempt("select * from x limit 10", true, sig1.clone());
        assert_eq!(out1, ProbeOutcomeKind::MeaningfulNewSignal);
        assert_eq!(
            st.probe_requirement_status(),
            ProbeRequirementStatus::Allowed
        );

        let sig2 = ProbeSignature::from_run_sql(
            "select * from y limit 10",
            &serde_json::json!({"ok":true}),
        );
        let out2 = st.note_probe_attempt("select * from y limit 10", true, sig2);
        assert_eq!(out2, ProbeOutcomeKind::MeaningfulNewSignal);
        assert_eq!(
            st.probe_requirement_status(),
            ProbeRequirementStatus::Allowed
        );

        let _ = st.note_probe_attempt("select * from x limit 10", true, sig1.clone());
        let _ = st.note_probe_attempt("select * from x limit 10", true, sig1.clone());
        let _ = st.note_probe_attempt("select * from x limit 10", true, sig1);
        let _ = st.note_probe_attempt(
            "select * from x limit 10",
            true,
            ProbeSignature::from_run_sql(
                "select * from x limit 10",
                &serde_json::json!({"ok":true}),
            ),
        );
        assert_eq!(
            st.probe_requirement_status(),
            ProbeRequirementStatus::ExhaustedRequireMutation
        );
    }

    #[test]
    fn successful_mutation_resets_probe_requirement_cycle() {
        let mut st = ExecutionState::new();
        st.telemetry.last_validate = Some(LastValidateState {
            ok: Some(false),
            ..LastValidateState::default()
        });
        st.telemetry.probe.required = true;
        st.note_patch_attempt(true, true);
        assert_eq!(
            st.probe_requirement_status(),
            ProbeRequirementStatus::NotRequired
        );
    }

    #[test]
    fn validate_and_failed_modes_are_set_via_controller_helpers() {
        let mut st = ExecutionState::new();
        st.enter_validate_mode(ExecutionTier::Cleanse);
        assert_eq!(st.phase.mode, ExecutionMode::Validate);
        assert_eq!(st.phase.current_tier, ExecutionTier::Cleanse);
        st.mark_failed("x");
        assert_eq!(st.phase.mode, ExecutionMode::Failed);
        assert_eq!(st.repair.last_error_brief.as_deref(), Some("x"));
    }

    #[test]
    fn publish_gate_requires_explicit_approval() {
        let mut st = ExecutionState::new();
        assert!(gate_publish_progress(&st, Phase::Publish).is_err());
        st.set_publish_approval(PublishApprovalDecision::Approved);
        assert!(gate_publish_progress(&st, Phase::PublishAwaitApproval).is_ok());
        assert!(gate_publish_progress(&st, Phase::Publish).is_ok());
        st.set_publish_approval(PublishApprovalDecision::Rejected);
        assert!(gate_publish_progress(&st, Phase::Publish).is_err());
    }

    #[test]
    fn publish_retry_budget_is_tracked_by_typed_kind() {
        let mut st = ExecutionState::new();
        assert_eq!(
            st.bump_publish_retry(PublishRetryKind::AwaitApprovalLoop, 3),
            1
        );
        assert_eq!(
            st.bump_publish_retry(PublishRetryKind::AwaitApprovalLoop, 3),
            2
        );
        assert_eq!(
            st.bump_publish_retry(PublishRetryKind::PublishFailureLoop, 3),
            1
        );
        st.reset_publish_retry(PublishRetryKind::AwaitApprovalLoop);
        assert_eq!(
            st.publish
                .publish_retries
                .iter()
                .find(|r| r.kind == PublishRetryKind::AwaitApprovalLoop)
                .map(|r| r.count),
            None
        );
    }

    #[test]
    fn repeated_equivalent_validate_failures_do_not_count_as_progress() {
        let mut st = ExecutionState::new();
        let sig = FailureSignature {
            class: FailureKind::SqlRuntime,
            node_id: Some("model.pkg.fct_orders".to_string()),
            canonical_path: Some(repair_target_path("models/marts/fct_orders.sql")),
            error_code: Some("E_SQL".to_string()),
        };
        let backlog = vec![RepairTarget {
            model_name: Some("model.pkg.fct_orders".to_string()),
            path: Some(repair_target_path("models/marts/fct_orders.sql")),
            error_class: Some(FailureKind::SqlRuntime),
            materialization: RepairTargetMaterialization::Existing,
        }];

        st.apply_validate_failure(
            ExecutionTier::Model,
            FailureKind::SqlRuntime,
            sig.clone(),
            RepairIntent::SqlPatch {
                target: sql_model_path("models/marts/fct_orders.sql"),
                backlog: backlog.clone(),
            },
            Some("first".to_string()),
        );
        let stall_after_first = st.repair.stall_count;

        st.apply_validate_failure(
            ExecutionTier::Model,
            FailureKind::SqlRuntime,
            sig,
            RepairIntent::SqlPatch {
                target: sql_model_path("models/marts/fct_orders.sql"),
                backlog,
            },
            Some("second".to_string()),
        );

        let delta = st.repair.last_progress_delta.expect("delta");
        assert!(!delta.progress_made);
        assert_eq!(delta.failed_target_count_delta, 0);
        assert!(!delta.failure_signature_changed);
        assert_eq!(st.repair.stall_count, stall_after_first.saturating_add(1));
    }

    #[test]
    fn shrinking_repair_backlog_counts_as_structural_progress() {
        let mut st = ExecutionState::new();
        let sig = FailureSignature {
            class: FailureKind::SqlRuntime,
            node_id: Some("model.pkg.fct_orders".to_string()),
            canonical_path: Some(repair_target_path("models/marts/fct_orders.sql")),
            error_code: Some("E_SQL".to_string()),
        };
        let two = vec![
            RepairTarget {
                model_name: Some("model.pkg.fct_orders".to_string()),
                path: Some(repair_target_path("models/marts/fct_orders.sql")),
                error_class: Some(FailureKind::SqlRuntime),
                materialization: RepairTargetMaterialization::Existing,
            },
            RepairTarget {
                model_name: Some("model.pkg.dim_users".to_string()),
                path: Some(repair_target_path("models/marts/dim_users.sql")),
                error_class: Some(FailureKind::SqlRuntime),
                materialization: RepairTargetMaterialization::Existing,
            },
        ];
        st.apply_validate_failure(
            ExecutionTier::Model,
            FailureKind::SqlRuntime,
            sig.clone(),
            RepairIntent::SqlPatch {
                target: sql_model_path("models/marts/fct_orders.sql"),
                backlog: two,
            },
            Some("first".to_string()),
        );

        let one = vec![RepairTarget {
            model_name: Some("model.pkg.fct_orders".to_string()),
            path: Some(repair_target_path("models/marts/fct_orders.sql")),
            error_class: Some(FailureKind::SqlRuntime),
            materialization: RepairTargetMaterialization::Existing,
        }];
        st.apply_validate_failure(
            ExecutionTier::Model,
            FailureKind::SqlRuntime,
            sig,
            RepairIntent::SqlPatch {
                target: sql_model_path("models/marts/fct_orders.sql"),
                backlog: one,
            },
            Some("second".to_string()),
        );

        let delta = st.repair.last_progress_delta.expect("delta");
        assert!(delta.progress_made);
        assert_eq!(delta.failed_target_count_delta, -1);
    }

    #[test]
    fn apply_event_batch_authoring_failed_enters_hard_repair_mode() {
        let mut st = ExecutionState::new();
        st.apply_event(DataEngineerEvent::BatchAuthoringFailed {
            tier: ExecutionTier::Cleanse,
            kind: FailureKind::SqlRuntime,
            failed_targets: vec![FailedModelRef {
                name: "AwsDataCatalog.test_raw.raw_customers".to_string(),
                file: "models/staging/stg_test_raw_raw_customers.sql".to_string(),
                materialization: RepairTargetMaterialization::Existing,
                ..Default::default()
            }],
            brief: "sql validation failed".to_string(),
        });
        assert!(st.hard_mutation_repair_mode());
        assert_eq!(st.repair_type(), RepairType::SqlTarget);
        assert_eq!(
            st.single_target_repair_path().as_deref(),
            Some("models/staging/stg_test_raw_raw_customers.sql")
        );
        assert_eq!(
            st.telemetry.last_validate.as_ref().and_then(|lv| lv.ok),
            Some(false)
        );
        assert_eq!(st.ladder_step(), RepairLadderStep::PatchTarget);
    }

    #[test]
    fn apply_event_batch_authoring_failed_missing_target_starts_with_replace_contents() {
        let mut st = ExecutionState::new();
        st.apply_event(DataEngineerEvent::BatchAuthoringFailed {
            tier: ExecutionTier::Cleanse,
            kind: FailureKind::SqlRuntime,
            failed_targets: vec![FailedModelRef {
                name: "AwsDataCatalog.test_raw.raw_orders".to_string(),
                file: "models/staging/stg_test_raw_raw_orders.sql".to_string(),
                materialization: RepairTargetMaterialization::Missing,
                ..Default::default()
            }],
            brief: "sql authoring never materialized target".to_string(),
        });
        assert!(st.hard_mutation_repair_mode());
        assert_eq!(st.repair_type(), RepairType::SqlTarget);
        assert_eq!(st.ladder_step(), RepairLadderStep::ReplaceContents);
        assert_eq!(
            st.single_target_repair_path().as_deref(),
            Some("models/staging/stg_test_raw_raw_orders.sql")
        );
    }

    #[test]
    fn apply_event_batch_authoring_failed_infra_transient_skips_repair() {
        let mut st = ExecutionState::new();
        st.apply_event(DataEngineerEvent::BatchAuthoringFailed {
            tier: ExecutionTier::Cleanse,
            kind: FailureKind::InfraTransient,
            failed_targets: vec![FailedModelRef {
                name: "AwsDataCatalog.test_raw.raw_customers".to_string(),
                file: "models/staging/stg_test_raw_raw_customers.sql".to_string(),
                materialization: RepairTargetMaterialization::Missing,
                ..Default::default()
            }],
            brief: "service error".to_string(),
        });
        assert!(
            !st.hard_mutation_repair_mode(),
            "infra-transient must not enter repair mode"
        );
        assert!(
            st.repair.last_batch_infra_transient,
            "flag must be set for step-boundary short-circuit"
        );
        assert_eq!(st.repair.stall_count, 0, "stall_count must not increment");
        assert_eq!(
            st.repair.last_error_brief.as_deref(),
            Some("service error")
        );
    }

    #[test]
    fn take_batch_infra_transient_clears_flag() {
        let mut repair = RepairState::default();
        repair.last_batch_infra_transient = true;
        assert!(repair.take_batch_infra_transient());
        assert!(!repair.last_batch_infra_transient);
        assert!(!repair.take_batch_infra_transient());
    }

    #[test]
    fn invariants_reject_patch_target_when_target_is_missing() {
        let mut st = ExecutionState::new();
        st.repair.repair_mode = RepairModeState::SqlTarget(SqlTargetRepairMode {
            target_path: sql_model_path("models/staging/stg_test.sql"),
            materialization: RepairTargetMaterialization::Missing,
            core: RepairModeCore {
                ladder_step: RepairLadderStep::PatchTarget,
                attempt_count: 0,
                repair_started_mutation_epoch: None,
                consecutive_noop_patches: 0,
            },
        });
        let err = st.validate_invariants().expect_err("invariants must fail");
        assert!(err.contains("cannot enter patch_target"));
    }

    #[test]
    fn apply_event_batch_authoring_recovered_clears_repair_mode() {
        let mut st = ExecutionState::new();
        st.repair.repair_mode = RepairModeState::SqlTarget(SqlTargetRepairMode {
            target_path: sql_model_path("models/staging/x.sql"),
            materialization: RepairTargetMaterialization::Existing,
            core: RepairModeCore {
                ladder_step: RepairLadderStep::PatchTarget,
                attempt_count: 0,
                repair_started_mutation_epoch: None,
                consecutive_noop_patches: 0,
            },
        });
        st.apply_event(DataEngineerEvent::BatchAuthoringRecovered);
        assert!(!st.hard_mutation_repair_mode());
        assert_eq!(st.repair_type(), RepairType::Unknown);
        assert!(st.single_target_repair_path().is_none());
    }

    #[test]
    fn invariants_reject_phase_reason_without_current_phase() {
        let mut st = ExecutionState::new();
        st.phase.current_phase = None;
        st.phase.phase_reason_code = Some(PhaseReasonCode::PhaseSet);
        let err = st.validate_invariants().expect_err("invariants must fail");
        assert!(err.contains("phase_reason_code set while current_phase is none"));
    }

    #[test]
    fn invariants_reject_repair_ladder_stop_without_required_attempts() {
        let mut st = ExecutionState::new();
        st.repair.repair_mode = RepairModeState::SqlTarget(SqlTargetRepairMode {
            target_path: sql_model_path("models/staging/stg_test.sql"),
            materialization: RepairTargetMaterialization::Existing,
            core: RepairModeCore {
                ladder_step: RepairLadderStep::Stop,
                attempt_count: 2,
                repair_started_mutation_epoch: None,
                consecutive_noop_patches: 0,
            },
        });
        let err = st.validate_invariants().expect_err("invariants must fail");
        assert!(err.contains("repair ladder reached stop before three attempts"));
    }

    #[test]
    fn invariants_reject_duplicate_publish_retry_entries() {
        let mut st = ExecutionState::new();
        st.publish.publish_retries = vec![
            PublishRetryState {
                kind: PublishRetryKind::AwaitApprovalLoop,
                count: 1,
            },
            PublishRetryState {
                kind: PublishRetryKind::AwaitApprovalLoop,
                count: 2,
            },
        ];
        let err = st.validate_invariants().expect_err("invariants must fail");
        assert!(err.contains("duplicate publish retry entry"));
    }

    #[test]
    fn invariants_reject_probe_required_when_last_validate_not_failed() {
        let mut st = ExecutionState::new();
        st.telemetry.last_validate = Some(LastValidateState {
            ok: Some(true),
            ..LastValidateState::default()
        });
        st.telemetry.probe.required = true;
        let err = st.validate_invariants().expect_err("invariants must fail");
        assert!(err.contains("probe.required can only be true"));
    }

    #[tokio::test]
    async fn load_strict_rejects_malformed_control_state() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let control = ControlStateStore::new(storage, scope, keyspace);
        let tid = "tid-malformed-control-state";

        let malformed_envelope = serde_json::json!({
            "schema_version": react_core::session::CONTROL_STATE_ENVELOPE_SCHEMA_VERSION,
            "suite_id": "data_engineer",
            "payload": {"schema_version":"bad"}
        });
        control
            .save(tid, "data_engineer", &malformed_envelope)
            .await
            .expect("seed control state");

        let got = ExecutionState::load_strict(&control, tid).await;
        assert!(got.is_err(), "malformed control_state must fail loudly");
    }
}
