use react_core::session::analysis::{Issue, IssueKind};
use react_core::session::{ThreadLog, ThreadStep, ToolStepStatus};
use react_core::suite::DebugProvider;

pub struct KbDebugProvider;

impl DebugProvider for KbDebugProvider {
    fn suite_id(&self) -> &'static str {
        "kb"
    }

    fn detect_domain_issues(&self, log: &ThreadLog) -> Vec<Issue> {
        let mut issues = Vec::new();
        detect_empty_search_results(log, &mut issues);
        detect_unnecessary_reingestion(log, &mut issues);
        issues
    }

    fn domain_context(&self) -> &'static str {
        "\
The KB (Knowledge Base) suite manages vector-indexed document collections.

## Tools
- kb_search(query, k?, dataset_id?): Searches the vector store for relevant documents. \
  Returns top-k results with scores.
- kb_ingest_dir(path, dataset_id?): Ingests documents from a directory into the vector store.

## Common Failure Patterns
- Empty search results: kb_search returning zero items usually means the dataset hasn't \
  been ingested yet or the query doesn't match any documents.
- Unnecessary re-ingestion: Calling kb_ingest_dir repeatedly on the same directory without \
  changes. Each call re-indexes all documents.
- Missing vector store: The suite requires a vector provider to be configured. If missing, \
  all operations will fail."
    }
}

fn detect_empty_search_results(log: &ThreadLog, issues: &mut Vec<Issue>) {
    for (idx, step) in log.steps.iter().enumerate() {
        if let ThreadStep::ToolEnd {
            name,
            status: ToolStepStatus::Ok,
            observation,
            ..
        } = step
        {
            if name == "kb_search" {
                let items_empty = observation
                    .extra
                    .get("items")
                    .and_then(|items| items.as_array())
                    .map_or(false, |arr| arr.is_empty());
                if items_empty {
                    issues.push(Issue {
                        kind: IssueKind::HighErrorRate,
                        description: "kb_search returned zero results".to_string(),
                        step_range: (idx, idx),
                    });
                }
            }
        }
    }
}

fn detect_unnecessary_reingestion(log: &ThreadLog, issues: &mut Vec<Issue>) {
    let mut ingest_count = 0;
    let mut first_ingest = 0;

    for (idx, step) in log.steps.iter().enumerate() {
        if let ThreadStep::ToolEnd { name, .. } = step {
            if name == "kb_ingest_dir" {
                if ingest_count == 0 {
                    first_ingest = idx;
                }
                ingest_count += 1;
            }
        }
    }

    if ingest_count >= 2 {
        issues.push(Issue {
            kind: IssueKind::Loop,
            description: format!("kb_ingest_dir called {} times in same thread", ingest_count),
            step_range: (first_ingest, log.steps.len().saturating_sub(1)),
        });
    }
}
