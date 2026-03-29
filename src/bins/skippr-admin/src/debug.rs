use std::sync::Arc;

use chrono::Utc;
use react_core::keyspace::Keyspace;
use react_core::session::analysis::{self, Issue, ThreadSummary};
use react_core::session::{ThreadLog, ThreadStep, ThreadStore, ToolStepStatus};
use react_core::suite::{DebugProviderRegistry, FlowFrame, Suite, SuiteCtx};
use react_suite_data_engineer::debug::DataEngineerDebugProvider;
use react_suite_debugger::SuiteDebugger;
use react_suite_kb::debug::KbDebugProvider;

pub fn build_debug_provider_registry() -> DebugProviderRegistry {
    let mut reg = DebugProviderRegistry::new();
    reg.register(DataEngineerDebugProvider);
    reg.register(KbDebugProvider);
    reg
}

pub async fn run_debug(
    thread_id: &str,
    suite: &SuiteDebugger,
    ctx: &SuiteCtx,
) -> Result<(), String> {
    let store = ThreadStore::new(
        ctx.storage().clone(),
        ctx.scope().clone(),
        ctx.keyspace().clone(),
    );

    let log = store
        .get(thread_id)
        .await
        .map_err(|e| format!("failed to read thread: {e}"))?;

    let summary = analysis::summarize(&log);

    let raw_log_excerpt = read_raw_log_excerpt(ctx.storage().as_ref(), ctx.keyspace().as_ref(), ctx.scope(), thread_id, 20_000).await;

    let evidence = build_evidence(thread_id, &log, &summary, &raw_log_excerpt);

    println!("  Analyzing thread with LLM...\n");

    let debug_tid = uuid::Uuid::new_v4().to_string();
    let question = format!(
        "Analyze thread {thread_id}. Here is the pre-gathered evidence:\n\n{evidence}\n\n\
         Provide a clear diagnosis with root cause, impact, and actionable fix recommendations."
    );

    let frames = suite
        .handle_new(&debug_tid, &question, "suite_debugger", ctx)
        .await?;

    let llm_analysis = extract_text(&frames);
    println!("\n{llm_analysis}\n");

    let report = format_report(thread_id, &summary, &evidence, &llm_analysis);
    let path = save_report(thread_id, &report)?;
    println!("  Report saved to {path}\n");

    let mut rl = rustyline::DefaultEditor::new().map_err(|e| e.to_string())?;
    loop {
        let readline = rl.readline("debug> ");
        match readline {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                if input == "/quit" || input == "quit" || input == "exit" {
                    break;
                }
                let _ = rl.add_history_entry(input);
                let frames = suite
                    .handle_user(&debug_tid, input, "suite_debugger", ctx)
                    .await?;
                let text = extract_text(&frames);
                println!("\n{text}\n");
            }
            Err(
                rustyline::error::ReadlineError::Interrupted
                | rustyline::error::ReadlineError::Eof,
            ) => {
                break;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

fn build_evidence(
    thread_id: &str,
    log: &ThreadLog,
    summary: &ThreadSummary,
    raw_log_excerpt: &str,
) -> String {
    let mut out = String::new();

    out.push_str(&format!("## Thread: {thread_id}\n\n"));

    out.push_str(&format!(
        "Steps: {}  |  LLM calls: {}  |  Duration: {}\n\n",
        summary.total_steps,
        summary.llm_calls,
        format_duration(summary.total_duration_ms),
    ));

    if !summary.phases.is_empty() {
        out.push_str("### Phases\n\n");
        out.push_str("| Phase | Steps | Duration |\n|---|---|---|\n");
        for p in &summary.phases {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                p.name,
                p.steps,
                format_duration(p.duration_ms),
            ));
        }
        out.push('\n');
    }

    if !summary.tool_calls.is_empty() {
        out.push_str("### Tool Usage\n\n");
        out.push_str("| Tool | Calls | OK | Fail |\n|---|---|---|---|\n");
        for t in &summary.tool_calls {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                t.name, t.count, t.successes, t.failures,
            ));
        }
        out.push('\n');
    }

    if !summary.issues.is_empty() {
        out.push_str("### Detected Issues\n\n");
        for issue in &summary.issues {
            out.push_str(&format!(
                "- **[{:?}]** {} (steps {}-{})\n",
                issue.kind, issue.description, issue.step_range.0, issue.step_range.1,
            ));
        }
        out.push('\n');
    }

    let key_ranges = collect_evidence_ranges(&summary.issues, log.steps.len());
    if !key_ranges.is_empty() {
        out.push_str("### Key Steps (around issues)\n\n");
        for (start, end) in &key_ranges {
            out.push_str(&format!("#### Steps {start}–{end}\n\n"));
            out.push_str("```\n");
            for i in *start..=*end {
                if i < log.steps.len() {
                    out.push_str(&format_step(i, &log.steps[i]));
                    out.push('\n');
                }
            }
            out.push_str("```\n\n");
        }
    }

    if !raw_log_excerpt.is_empty() {
        out.push_str("### Raw Log Excerpt (last 20KB)\n\n");
        out.push_str("```\n");
        out.push_str(raw_log_excerpt);
        out.push_str("\n```\n\n");
    }

    out
}

fn collect_evidence_ranges(issues: &[Issue], total_steps: usize) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();

    for issue in issues {
        let (s, e) = issue.step_range;
        let start = s.saturating_sub(2);
        let end = (e + 2).min(total_steps.saturating_sub(1));
        ranges.push((start, end));
    }

    ranges.sort();
    ranges.dedup();

    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in ranges {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 + 1 {
                last.1 = last.1.max(e);
                continue;
            }
        }
        merged.push((s, e));
    }

    const MAX_EVIDENCE_STEPS: usize = 60;
    let total: usize = merged.iter().map(|(s, e)| e - s + 1).sum();
    if total > MAX_EVIDENCE_STEPS {
        merged.truncate(5);
    }

    merged
}

fn format_step(idx: usize, step: &ThreadStep) -> String {
    match step {
        ThreadStep::Phase {
            phase,
            from_phase,
            ts,
            ..
        } => {
            let from = from_phase
                .as_deref()
                .map(|f| format!(" (from {f})"))
                .unwrap_or_default();
            format!("[{idx}] {ts} PHASE {phase}{from}")
        }
        ThreadStep::ToolEnd {
            name,
            status,
            args,
            observation,
            ts,
            ..
        } => {
            let status_str = match status {
                ToolStepStatus::Ok => "OK",
                ToolStepStatus::Failed => "FAIL",
                ToolStepStatus::Running => "RUNNING",
            };
            let args_short = truncate_json(args, 200);
            let obs_short = truncate_json(&serde_json::to_value(observation).unwrap_or_default(), 300);
            format!("[{idx}] {ts} TOOL_END {name} [{status_str}] args={args_short} obs={obs_short}")
        }
        ThreadStep::ToolStart {
            name, args, ts, ..
        } => {
            let args_short = truncate_json(args, 200);
            format!("[{idx}] {ts} TOOL_START {name} args={args_short}")
        }
        ThreadStep::LlmStart {
            phase, model, ts, ..
        } => {
            let m = model.as_deref().unwrap_or("?");
            format!("[{idx}] {ts} LLM_START phase={phase} model={m}")
        }
        ThreadStep::LlmEnd {
            phase, status, ts, ..
        } => {
            format!("[{idx}] {ts} LLM_END phase={phase} status={status:?}")
        }
        ThreadStep::User { text, ts, .. } => {
            let short = if text.len() > 100 {
                format!("{}…", &text[..100])
            } else {
                text.clone()
            };
            format!("[{idx}] {ts} USER {short}")
        }
        _ => {
            format!("[{idx}] {:?}", std::mem::discriminant(step))
        }
    }
}

fn truncate_json(v: &serde_json::Value, max_len: usize) -> String {
    let s = serde_json::to_string(v).unwrap_or_else(|_| "null".to_string());
    if s.len() > max_len {
        format!("{}…", &s[..max_len])
    } else {
        s
    }
}

fn format_duration(ms: Option<i64>) -> String {
    match ms {
        Some(ms) => format!("{:.1}s", ms as f64 / 1000.0),
        None => "?".to_string(),
    }
}

async fn read_raw_log_excerpt(
    storage: &dyn react_core::storage::StorageAdapter,
    keyspace: &dyn Keyspace,
    scope: &react_core::scope::RequestScope,
    thread_id: &str,
    max_bytes: usize,
) -> String {
    let key = match keyspace.thread_log_key(scope, thread_id) {
        Ok(k) => k,
        Err(_) => return String::new(),
    };
    match storage.get_bytes(&key).await {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            if text.len() > max_bytes {
                text[text.len() - max_bytes..].to_string()
            } else {
                text.to_string()
            }
        }
        Err(_) => String::new(),
    }
}

fn format_report(
    thread_id: &str,
    summary: &ThreadSummary,
    evidence: &str,
    llm_analysis: &str,
) -> String {
    let mut report = String::new();
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");

    report.push_str(&format!("# Debug Report: {thread_id}\n\n"));
    report.push_str(&format!("Generated: {now}\n\n"));
    report.push_str(&format!(
        "Result: {}\n\n",
        summary.result.as_deref().unwrap_or("unknown"),
    ));

    report.push_str("---\n\n## LLM Analysis\n\n");
    report.push_str(llm_analysis);
    report.push_str("\n\n---\n\n## Evidence\n\n");
    report.push_str(evidence);

    report
}

fn save_report(thread_id: &str, report: &str) -> Result<String, String> {
    let dir = std::path::PathBuf::from("debug-reports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create debug-reports/: {e}"))?;

    let ts = Utc::now().format("%Y%m%dT%H%M%SZ");
    let short_id = &thread_id[..thread_id.len().min(8)];
    let filename = format!("{ts}_{short_id}.md");
    let path = dir.join(&filename);

    std::fs::write(&path, report).map_err(|e| format!("failed to write report: {e}"))?;
    Ok(path.display().to_string())
}

fn extract_text(frames: &[FlowFrame]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for frame in frames {
        match frame {
            FlowFrame::Complete {
                display, payload, ..
            } => {
                if let Some(text) = display {
                    parts.push(text.clone());
                } else if let Some(text) = payload.as_str() {
                    parts.push(text.to_string());
                } else {
                    parts.push(serde_json::to_string_pretty(payload).unwrap_or_default());
                }
            }
            FlowFrame::Interrupt { prompt, .. } => {
                parts.push(format!("[Interrupt] {prompt}"));
            }
            _ => {}
        }
    }
    parts.join("\n")
}

pub fn wire_debug_capabilities(ctx: &mut SuiteCtx) {
    let registry = build_debug_provider_registry();
    ctx.set_capability(Arc::new(registry));
}
