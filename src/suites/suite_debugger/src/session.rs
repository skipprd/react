use serde::{Deserialize, Serialize};

use react_core::session::ThreadResult;

const MAX_PRIOR_FINDINGS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DebugSession {
    pub target_thread_id: String,
    pub target_suite_id: Option<String>,
    pub prior_findings: Vec<String>,
}

impl DebugSession {
    pub fn new(target_thread_id: String, target_suite_id: Option<String>) -> Self {
        Self {
            target_thread_id,
            target_suite_id,
            prior_findings: Vec::new(),
        }
    }

    pub fn extract_findings(&mut self, result: &ThreadResult) {
        let display = result
            .display
            .as_deref()
            .unwrap_or_else(|| result.payload.as_str().unwrap_or("(no display)"));

        let trimmed = if display.len() > 2000 {
            format!("{}...", &display[..2000])
        } else {
            display.to_string()
        };

        self.prior_findings.push(trimmed);
        while self.prior_findings.len() > MAX_PRIOR_FINDINGS {
            self.prior_findings.remove(0);
        }
    }

    pub fn build_context_block(&self) -> String {
        if self.prior_findings.is_empty() {
            return String::new();
        }
        let mut out = String::from("\n## Prior findings from this debug session:\n");
        for (i, f) in self.prior_findings.iter().enumerate() {
            out.push_str(&format!("\n### Turn {}\n{}\n", i + 1, f));
        }
        out
    }
}
