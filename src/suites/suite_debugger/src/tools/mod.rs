pub mod admin_repo_query;
pub mod list_threads;
pub mod read_log;
pub mod read_thread;
pub mod summarize;

use react_core::tools::ToolRegistry;

pub fn register_all(registry: &mut ToolRegistry, enable_admin_repo_query: bool) {
    registry.register(list_threads::ListThreadsTool);
    registry.register(read_thread::ReadThreadTool);
    registry.register(read_log::ReadRunLogTool);
    registry.register(summarize::SummarizeThreadTool);
    if enable_admin_repo_query {
        registry.register(admin_repo_query::AdminRepoQueryTool);
    }
}

pub fn tool_card(enable_admin_repo_query: bool) -> String {
    let mut card = String::from(
        r#"Available tools:
- list_threads(): Lists all thread IDs in the current scope. Returns { thread_ids: [...], count: N }.
- read_thread(thread_id): Reads the full ThreadLog for a thread. Returns steps with type, timestamps, tool names, statuses. Each step has an _index field.
- read_run_log(thread_id, max_chars?): Reads the plain-text run log for a thread. Returns raw log output. Optional max_chars (default 50000).
- summarize_thread(thread_id): Returns a structured ThreadSummary with phases, tool call counts, LLM call counts, duration, and detected issues (loops, stalls, high error rates, excessive LLM calls, phase regressions)."#,
    );

    if enable_admin_repo_query {
        card.push_str(
            "\n- admin_repo_query(query, k?): Searches admin-only repo embeddings for relevant code and docs. Use this when thread evidence points to product/runtime behavior that needs code-level confirmation.",
        );
    }
    card.push_str(
        "\n\nTo debug a thread: start with summarize_thread to get an overview, then drill into specific steps with read_thread, check raw logs with read_run_log if needed, and use admin_repo_query when repo context would strengthen an evidence-backed conclusion.",
    );
    card
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_repo_query_only_registers_when_enabled() {
        let mut base = ToolRegistry::new();
        register_all(&mut base, false);
        assert!(!base.contains("admin_repo_query"));

        let mut admin = ToolRegistry::new();
        register_all(&mut admin, true);
        assert!(admin.contains("admin_repo_query"));
    }
}
