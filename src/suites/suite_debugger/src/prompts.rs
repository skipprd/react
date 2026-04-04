use react_core::session::analysis::ThreadSummary;

pub fn system_prompt(
    summary: &ThreadSummary,
    domain_context: Option<&str>,
    strict_audit: bool,
) -> String {
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

    if strict_audit {
        prompt.push_str(
            "\n## Audit Requirements\n\
             - Treat this as a forensic audit, not a generic advice session.\n\
             - Establish whether the thread completed, partially completed, or failed.\n\
             - Explain efficiency: what consumed time, what repeated, and what was unnecessary.\n\
             - Identify the single primary root cause and any secondary contributing factors.\n\
             - When discussing fixes, tie them to concrete evidence from phases, tool calls, log excerpts, or repo code retrieved via tools.\n\
             - Do not speculate when evidence is missing; explicitly say what you verified versus what remains uncertain.\n",
        );
    }

    if let Ok(json) = serde_json::to_string_pretty(summary) {
        prompt.push_str(&json);
    }

    if let Some(dc) = domain_context {
        prompt.push_str("\n\n## Domain Context\n");
        prompt.push_str(dc);
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_audit_prompt_adds_forensic_requirements() {
        let summary = ThreadSummary {
            total_steps: 0,
            phases: vec![],
            llm_calls: 0,
            tool_calls: vec![],
            total_duration_ms: None,
            issues: vec![],
            result: None,
        };
        let prompt = system_prompt(&summary, None, true);
        assert!(prompt.contains("Treat this as a forensic audit"));
        assert!(prompt.contains("Establish whether the thread completed"));
    }
}
