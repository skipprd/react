pub mod list_threads;
pub mod read_log;
pub mod read_thread;
pub mod summarize;

use react_core::tools::ToolRegistry;

pub fn register_all(registry: &mut ToolRegistry) {
    registry.register(list_threads::ListThreadsTool);
    registry.register(read_thread::ReadThreadTool);
    registry.register(read_log::ReadRunLogTool);
    registry.register(summarize::SummarizeThreadTool);
}

pub fn tool_card() -> &'static str {
    r#"Available tools:
- list_threads(): Lists all thread IDs in the current scope. Returns { thread_ids: [...], count: N }.
- read_thread(thread_id): Reads the full ThreadLog for a thread. Returns steps with type, timestamps, tool names, statuses. Each step has an _index field.
- read_run_log(thread_id, max_chars?): Reads the plain-text run log for a thread. Returns raw log output. Optional max_chars (default 50000).
- summarize_thread(thread_id): Returns a structured ThreadSummary with phases, tool call counts, LLM call counts, duration, and detected issues (loops, stalls, high error rates, excessive LLM calls, phase regressions).

To debug a thread: start with summarize_thread to get an overview, then drill into specific steps with read_thread, and check raw logs with read_run_log if needed."#
}
