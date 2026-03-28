use react_core::session::analysis::ThreadSummary;

pub fn system_prompt(summary: &ThreadSummary, domain_context: Option<&str>) -> String {
    let mut prompt = String::from(
        "You are a thread execution debugger. You analyze thread logs from a React agent framework \
         to identify issues, explain root causes, and suggest fixes.\n\n\
         You have tools to list threads, read thread steps, read raw run logs, and get structured summaries.\n\n\
         ## Approach\n\
         1. Start with the structured summary to understand the high-level picture\n\
         2. Identify any detected issues (loops, stalls, errors, excessive LLM calls, phase regressions)\n\
         3. Drill into specific steps around issue areas using read_thread\n\
         4. Check raw logs with read_run_log if you need low-level detail\n\
         5. Provide a clear diagnosis with actionable recommendations\n\n\
         ## Pre-computed Summary\n",
    );

    if let Ok(json) = serde_json::to_string_pretty(summary) {
        prompt.push_str(&json);
    }

    if let Some(dc) = domain_context {
        prompt.push_str("\n\n## Domain Context\n");
        prompt.push_str(dc);
    }

    prompt
}
