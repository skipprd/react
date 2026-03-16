use serde::{Deserialize, Serialize};

/// Accumulated history of a repair session, ensuring every iteration builds on all prior context.
///
/// `format_for_prompt()` serialises the full history so that prompts are never identical across
/// iterations — the LLM always sees prior attempts and their outcomes.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RepairSessionLog {
    pub error_context: RepairErrorContext,
    pub iterations: Vec<RepairIteration>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RepairErrorContext {
    pub validate_brief: String,
    pub failing_tests: Vec<FailingTest>,
    pub failed_models: Vec<FailedModelBrief>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FailingTest {
    pub name: String,
    pub error_snippet: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FailedModelBrief {
    pub name: String,
    pub file: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RepairIteration {
    pub index: usize,
    pub gathered_files: Vec<(String, String)>,
    pub diagnosis: String,
    pub planned_fixes: Vec<PlannedFix>,
    pub apply_results: Vec<ApplyResult>,
    pub validate_outcome: Option<ValidateOutcome>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlannedFix {
    pub path: String,
    pub op: FileOp,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileOp {
    Patch,
    Write,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplyResult {
    pub path: String,
    pub op: FileOp,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidateOutcome {
    pub passed: bool,
    pub error_summary: String,
}

impl RepairSessionLog {
    pub fn new(error_context: RepairErrorContext) -> Self {
        Self {
            error_context,
            iterations: Vec::new(),
        }
    }

    pub fn record(
        &mut self,
        index: usize,
        gathered_files: Vec<(String, String)>,
        diagnosis: String,
        planned_fixes: Vec<PlannedFix>,
        apply_results: Vec<ApplyResult>,
    ) {
        self.iterations.push(RepairIteration {
            index,
            gathered_files,
            diagnosis,
            planned_fixes,
            apply_results,
            validate_outcome: None,
        });
    }

    pub fn record_validate(&mut self, outcome: ValidateOutcome) {
        if let Some(last) = self.iterations.last_mut() {
            last.validate_outcome = Some(outcome);
        }
    }

    pub fn last(&self) -> Option<&RepairIteration> {
        self.iterations.last()
    }

    /// Serialises the full session history into a prompt-ready string.
    /// Each iteration is described completely so the LLM can learn from prior attempts.
    pub fn format_for_prompt(&self) -> String {
        let mut out = String::new();

        out.push_str("## Error Context\n\n");
        out.push_str(&self.error_context.validate_brief);
        out.push('\n');

        if !self.error_context.failing_tests.is_empty() {
            out.push_str("\n### Failing Tests\n\n");
            for t in &self.error_context.failing_tests {
                out.push_str(&format!("- **{}**: {}\n", t.name, t.error_snippet));
            }
        }

        if !self.error_context.failed_models.is_empty() {
            out.push_str("\n### Failed Models\n\n");
            for m in &self.error_context.failed_models {
                out.push_str(&format!(
                    "- **{}** (`{}`): {}\n",
                    m.name,
                    m.file,
                    m.error.as_deref().unwrap_or("no error detail")
                ));
            }
        }

        if !self.iterations.is_empty() {
            out.push_str("\n## Prior Repair Attempts\n\n");
            for iter in &self.iterations {
                out.push_str(&format!("### Iteration {}\n\n", iter.index + 1));

                if !iter.gathered_files.is_empty() {
                    out.push_str("**Files read:**\n");
                    for (path, _) in &iter.gathered_files {
                        out.push_str(&format!("- `{}`\n", path));
                    }
                }

                if !iter.diagnosis.is_empty() {
                    out.push_str(&format!("\n**Diagnosis:** {}\n", iter.diagnosis));
                }

                if !iter.planned_fixes.is_empty() {
                    out.push_str("\n**Planned fixes:**\n");
                    for fix in &iter.planned_fixes {
                        out.push_str(&format!("- `{}` ({:?})\n", fix.path, fix.op));
                    }
                }

                if !iter.apply_results.is_empty() {
                    out.push_str("\n**Apply results:**\n");
                    for r in &iter.apply_results {
                        let status = if r.success { "OK" } else { "FAILED" };
                        let err = r
                            .error
                            .as_deref()
                            .map(|e| format!(" — {}", e))
                            .unwrap_or_default();
                        out.push_str(&format!(
                            "- `{}` ({:?}): {}{}\n",
                            r.path, r.op, status, err
                        ));
                    }
                }

                if let Some(v) = &iter.validate_outcome {
                    let label = if v.passed { "PASSED" } else { "FAILED" };
                    out.push_str(&format!(
                        "\n**Validation:** {} — {}\n",
                        label, v.error_summary
                    ));
                }
                out.push('\n');
            }
        }

        out
    }
}

impl RepairIteration {
    pub fn has_mutations(&self) -> bool {
        self.apply_results.iter().any(|r| r.success)
    }

    pub fn error_brief(&self) -> String {
        self.validate_outcome
            .as_ref()
            .map(|v| v.error_summary.clone())
            .unwrap_or_default()
    }

    pub fn files_changed(&self) -> Vec<String> {
        self.apply_results
            .iter()
            .filter(|r| r.success)
            .map(|r| r.path.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_for_prompt_includes_iterations() {
        let mut log = RepairSessionLog::new(RepairErrorContext {
            validate_brief: "dbt test failed: 2 failures".into(),
            failing_tests: vec![FailingTest {
                name: "not_null_orders_id".into(),
                error_snippet: "got 3 results".into(),
            }],
            failed_models: vec![],
        });

        log.record(
            0,
            vec![("models/orders.sql".into(), "SELECT ...".into())],
            "null ids in orders table".into(),
            vec![PlannedFix {
                path: "models/orders.sql".into(),
                op: FileOp::Patch,
                content: "...".into(),
            }],
            vec![ApplyResult {
                path: "models/orders.sql".into(),
                op: FileOp::Patch,
                success: false,
                error: Some("patch_hunk_context_miss".into()),
            }],
        );

        let prompt = log.format_for_prompt();
        assert!(prompt.contains("Iteration 1"));
        assert!(prompt.contains("patch_hunk_context_miss"));
        assert!(prompt.contains("not_null_orders_id"));
        assert!(prompt.contains("dbt test failed"));
    }

    #[test]
    fn empty_session_produces_context_only() {
        let log = RepairSessionLog::new(RepairErrorContext {
            validate_brief: "compile error".into(),
            failing_tests: vec![],
            failed_models: vec![],
        });
        let prompt = log.format_for_prompt();
        assert!(prompt.contains("compile error"));
        assert!(!prompt.contains("Prior Repair Attempts"));
    }
}
