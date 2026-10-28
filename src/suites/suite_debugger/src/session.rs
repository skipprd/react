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

        let trimmed = react_core::text::truncate_chars_ellipsis(display, 2000);

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

#[cfg(test)]
mod tests {
    use super::DebugSession;
    use react_core::session::ThreadResult;

    #[test]
    fn extract_findings_does_not_panic_on_utf8_boundary() {
        let mut s = DebugSession::new("tid".into(), None);
        s.extract_findings(&ThreadResult {
            kind: "completed".into(),
            payload: serde_json::json!({}),
            display: Some("é".repeat(2001)),
        });
        assert_eq!(s.prior_findings.len(), 1);
        assert!(s.prior_findings[0].ends_with("..."));
        assert_eq!(s.prior_findings[0].chars().count(), 2003); // 2000 + "..."
    }
}
