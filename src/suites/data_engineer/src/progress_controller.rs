use crate::failure_kind::FailureKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fmt;

use crate::domain_types::PhaseReasonCode;
use react_core::session::ControlStateStore;

use crate::control_flow::Phase;

pub const EXECUTION_STATE_SCHEMA_VERSION: u32 = 2;
pub const MAX_REPAIR_CYCLES: usize = 8;

#[derive(Clone, Debug)]
pub struct RepairPromptContext {
    pub brief: Option<String>,
    pub log_excerpts: Option<String>,
    pub repair_cycles: usize,
    pub guard_note: Option<String>,
    pub recent_failed_file_ops: Vec<RecentFailedFileOp>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentFailedFileOp {
    pub op: String,
    pub path: String,
    pub error_brief: String,
    pub count: usize,
}

impl RepairPromptContext {
    pub fn format_error_context(&self) -> String {
        let mut out = String::new();
        if let Some(ref brief) = self.brief {
            let trimmed = brief.trim();
            if !trimmed.is_empty() {
                out.push_str("Last dbt_validate summary:\n");
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
            let mut total_failure_count: usize = 0;
            for item in self.recent_failed_file_ops.iter().take(8) {
                total_failure_count += item.count;
                if item.count > 1 {
                    out.push_str(&format!(
                        "- file op='{}' path='{}' failed {} times: {}\n",
                        item.op, item.path, item.count, item.error_brief
                    ));
                } else {
                    out.push_str(&format!(
                        "- file op='{}' path='{}' failed: {}\n",
                        item.op, item.path, item.error_brief
                    ));
                }
            }
            out.push_str(
                "If a prior patch failed, read the exact current file content and make a materially different edit.\n",
            );
            for item in self.recent_failed_file_ops.iter().take(8) {
                if item.count >= 3 {
                    out.push_str(&format!(
                        "\nBLOCKED: file op='{}' path='{}' has failed {} times with the same error. \
                         You MUST NOT attempt the same operation again. Re-read the error and take a completely different approach \
                         (e.g. rename the file with op=mv, change the source reference, or use op=write instead of op=patch).\n",
                        item.op, item.path, item.count
                    ));
                }
            }
            if total_failure_count >= 3 {
                out.push_str(&format!(
                    "\nYou have {} recent failed file mutations. You MUST take a fundamentally different approach.\n\
                     If patching a YAML file keeps failing, consider using op=write with the complete correct file content.\n\
                     If the same path validation error repeats, re-read the error and change your approach entirely.\n",
                    total_failure_count
                ));
            }
        }
        let has_repair_data = self.brief.is_some() || !self.recent_failed_file_ops.is_empty();
        if has_repair_data {
            out.push_str(
                "\nCRITICAL REPAIR RULES:\n\
                 - You MUST change the SQL logic or test definition to fix the actual error described above.\n\
                 - Read the error message carefully: identify the failing column/expression, then edit \
                 the SQL model to produce correct values (filter NULLs, fix joins, cast types, etc.).\n\
                 - Use your tools (file list, file read) to identify which model(s) or test(s) need fixing.\n",
            );
        }
        if self.repair_cycles >= 2 {
            out.push_str(&format!(
                "\nWARNING: Repair cycle {} of {}. Previous attempts did NOT fully resolve the issue. \
                 You MUST take a materially different approach.\n",
                self.repair_cycles, MAX_REPAIR_CYCLES,
            ));
        }
        out
    }

    pub fn has_context(&self) -> bool {
        self.brief.is_some()
            || self.guard_note.is_some()
            || !self.recent_failed_file_ops.is_empty()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LastValidateState {
    #[serde(default)]
    pub ok: Option<bool>,
    #[serde(default)]
    pub compile_ok: Option<bool>,
    #[serde(default)]
    pub run_ok: Option<bool>,
    #[serde(default)]
    pub brief: Option<String>,
    #[serde(default)]
    pub log_excerpts: Option<String>,
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


#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SubjectiveRetryKind {
    PlanSemanticInvalid,
    PlanGroundingEmptyAfterPrune,
    PlanGroundingStagingDiscoveryEmpty,
    ReviewPatchImpl,
    ReviewPlanChange,
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
pub struct RepairState {
    #[serde(default)]
    pub mutation_epoch: u64,
    /// Epoch at which the last validate failure occurred; `mutation_epoch > fail_mutation_epoch`
    /// means the agent has mutated since the last failure.
    #[serde(default)]
    pub fail_mutation_epoch: u64,
    #[serde(default)]
    pub repair_cycles: usize,
    #[serde(default)]
    pub last_error_brief: Option<String>,
    #[serde(default)]
    pub pending_patch_impl: Option<PatchImplIntent>,
    #[serde(default)]
    pub repair_active: bool,
    #[serde(default)]
    pub last_batch_infra_transient: bool,
}

impl RepairState {
    pub fn hard_mutation_repair_mode(&self) -> bool {
        self.repair_active
    }

    pub fn mutated_since_fail(&self) -> bool {
        self.mutation_epoch > self.fail_mutation_epoch
    }

    pub fn take_batch_infra_transient(&mut self) -> bool {
        std::mem::take(&mut self.last_batch_infra_transient)
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

        self.repair.repair_cycles = 0;
        self.repair.fail_mutation_epoch = 0;
        self.repair.last_error_brief = None;
        self.repair.pending_patch_impl = None;
        self.repair.repair_active = false;

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
        brief: String,
        failure_hash: String,
        compile_ok: bool,
        run_ok: bool,
        log_excerpts: Option<String>,
    },
    BatchAuthoringFailed {
        tier: ExecutionTier,
        kind: FailureKind,
        brief: String,
    },
    BatchAuthoringRecovered,
}

impl ExecutionState {
    pub fn repair_prompt_context(&self) -> RepairPromptContext {
        let lv = self.telemetry.last_validate.as_ref();
        let brief = lv
            .and_then(|lv| lv.brief.as_ref())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let log_excerpts = lv
            .and_then(|lv| lv.log_excerpts.as_ref())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let guard_note = self
            .repair
            .last_error_brief
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        RepairPromptContext {
            brief,
            log_excerpts,
            repair_cycles: self.repair.repair_cycles,
            guard_note,
            recent_failed_file_ops: Vec::new(),
        }
    }

    fn last_validate_failed(&self) -> bool {
        self.telemetry.last_validate.as_ref().and_then(|lv| lv.ok) == Some(false)
    }

    pub fn hard_mutation_repair_mode(&self) -> bool {
        self.repair_state().hard_mutation_repair_mode()
    }

    pub fn new() -> Self {
        Self {
            schema_version: EXECUTION_STATE_SCHEMA_VERSION,
            ..Default::default()
        }
    }

    pub fn apply_validate_success(&mut self, tier: ExecutionTier) {
        self.telemetry.last_validate = Some(LastValidateState {
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
        brief: String,
        _failure_hash: String,
        compile_ok: bool,
        run_ok: bool,
        log_excerpts: Option<String>,
    ) {
        self.telemetry.last_validate = Some(LastValidateState {
            ok: Some(false),
            compile_ok: Some(compile_ok),
            run_ok: Some(run_ok),
            brief: Some(brief.clone()),
            log_excerpts,
        });
        self.with_workflow_control_state_mut(|state| {
            state.phase.current_tier = tier;
            state.phase.mode = ExecutionMode::Mutate;

            state.repair.last_error_brief = Some(brief);
            state.repair.repair_active = true;
            state.repair.repair_cycles = state.repair.repair_cycles.saturating_add(1);
            state.repair.fail_mutation_epoch = state.repair.mutation_epoch;

            state.publish.publish_approval = None;
            state.probe = ProbeState::default();
            state.probe.required = compile_ok;
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

    pub fn apply_event(&mut self, event: DataEngineerEvent) {
        match event {
            DataEngineerEvent::ValidatePassed { tier } => self.apply_validate_success(tier),
            DataEngineerEvent::ValidateFailed {
                tier,
                brief,
                failure_hash,
                compile_ok,
                run_ok,
                log_excerpts,
            } => {
                self.apply_validate_failure(tier, brief, failure_hash, compile_ok, run_ok, log_excerpts);
            }
            DataEngineerEvent::BatchAuthoringFailed { tier, kind, brief } => {
                if kind == FailureKind::InfraTransient {
                    self.repair.last_batch_infra_transient = true;
                    self.repair.last_error_brief = Some(brief);
                    self.debug_assert_invariants();
                    return;
                }
                let hash = sha256_hex(brief.trim());
                self.apply_validate_failure(tier, brief, hash, false, false, None);
            }
            DataEngineerEvent::BatchAuthoringRecovered => {
                self.repair.repair_active = false;
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

pub fn sha256_hex(input: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
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

pub fn gate_authoring_probe(state: &ExecutionState) -> Result<(), String> {
    match state.probe_requirement_status() {
        ProbeRequirementStatus::Required => Err(
            "progress_gate_blocked: runtime validation previously failed after compile and a meaningful data probe is still required"
                .to_string(),
        ),
        ProbeRequirementStatus::ExhaustedRequireMutation => Err(
            "progress_gate_blocked: probe loop exhausted (repeated/no-new-signal probes); apply a mutating fix before validating"
                .to_string(),
        ),
        ProbeRequirementStatus::NotRequired | ProbeRequirementStatus::Allowed => Ok(()),
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
    fn repair_prompt_context_includes_recent_failed_file_ops() {
        let ctx = RepairPromptContext {
            brief: None,
            log_excerpts: None,
            repair_cycles: 0,
            guard_note: None,
            recent_failed_file_ops: vec![RecentFailedFileOp {
                op: "patch".to_string(),
                path: "models/staging/stg_orders.yml".to_string(),
                error_brief: "invalid YAML: duplicate entry with key \"version\"".to_string(),
                count: 1,
            }],
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
        st.repair.repair_active = true;
        st.repair.last_error_brief = Some("some error".to_string());
        st.subjective_retries
            .insert(SubjectiveRetryKind::PlanSemanticInvalid, 3);
        st.subjective_retries
            .insert(SubjectiveRetryKind::ValidatePrecheckFailed, 2);
        st.apply_validate_success(ExecutionTier::Model);
        assert_eq!(st.phase.current_tier, ExecutionTier::Model);
        assert_eq!(st.phase.mode, ExecutionMode::Done);
        assert!(!st.hard_mutation_repair_mode());
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
    fn validate_failure_activates_repair() {
        let mut st = ExecutionState::new();
        st.apply_validate_failure(
            ExecutionTier::Cleanse,
            "schema fail".to_string(),
            sha256_hex("schema fail"),
            true,
            false,
            None,
        );
        assert!(st.hard_mutation_repair_mode());
        assert!(st.repair.repair_active);
        assert_eq!(st.repair.last_error_brief.as_deref(), Some("schema fail"));
        assert_eq!(st.repair.repair_cycles, 1);
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
    fn gate_authoring_probe_rejects_when_probe_required() {
        let mut st = ExecutionState::new();
        st.telemetry.last_validate = Some(LastValidateState {
            ok: Some(false),
            compile_ok: Some(true),
            run_ok: Some(false),
            ..LastValidateState::default()
        });
        st.telemetry.probe.required = true;
        assert!(gate_authoring_probe(&st).is_err());
    }

    #[test]
    fn gate_authoring_probe_accepts_when_no_probe_required() {
        let mut st = ExecutionState::new();
        st.repair.repair_active = true;
        st.telemetry.last_validate = Some(LastValidateState {
            compile_ok: Some(true),
            run_ok: Some(true),
            ..LastValidateState::default()
        });
        st.telemetry.probe.required = false;
        assert!(gate_authoring_probe(&st).is_ok());
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
        st.telemetry.probe.required = false;
        st.telemetry.probe.repeated_signature_streak = 0;
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
    fn validate_failures_increment_repair_cycles() {
        let mut st = ExecutionState::new();

        st.apply_validate_failure(
            ExecutionTier::Model,
            "error A".to_string(),
            sha256_hex("error A"),
            true,
            false,
            None,
        );
        assert_eq!(st.repair.repair_cycles, 1);

        st.apply_validate_failure(
            ExecutionTier::Model,
            "error A".to_string(),
            sha256_hex("error A"),
            true,
            false,
            None,
        );
        assert_eq!(st.repair.repair_cycles, 2);

        st.apply_validate_failure(
            ExecutionTier::Model,
            "error B".to_string(),
            sha256_hex("error B"),
            true,
            false,
            None,
        );
        assert_eq!(st.repair.repair_cycles, 3, "different errors still increment");
    }

    #[test]
    fn apply_event_batch_authoring_failed_activates_repair() {
        let mut st = ExecutionState::new();
        st.apply_event(DataEngineerEvent::BatchAuthoringFailed {
            tier: ExecutionTier::Cleanse,
            kind: FailureKind::Unknown,
            brief: "sql validation failed".to_string(),
        });
        assert!(st.hard_mutation_repair_mode());
        assert_eq!(
            st.telemetry.last_validate.as_ref().and_then(|lv| lv.ok),
            Some(false)
        );
    }

    #[test]
    fn apply_event_batch_authoring_failed_infra_transient_skips_repair() {
        let mut st = ExecutionState::new();
        st.apply_event(DataEngineerEvent::BatchAuthoringFailed {
            tier: ExecutionTier::Cleanse,
            kind: FailureKind::InfraTransient,
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
        assert_eq!(st.repair.repair_cycles, 0, "repair_cycles must not increment");
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
    fn apply_event_batch_authoring_recovered_clears_repair() {
        let mut st = ExecutionState::new();
        st.repair.repair_active = true;
        st.repair.last_error_brief = Some("some error".to_string());
        st.apply_event(DataEngineerEvent::BatchAuthoringRecovered);
        assert!(!st.hard_mutation_repair_mode());
        assert!(st.repair.last_error_brief.is_none());
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
