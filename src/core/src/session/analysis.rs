use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::{LlmStepStatus, ThreadLog, ThreadStep, ToolStepStatus};

#[derive(Debug, Clone, Serialize)]
pub struct ThreadSummary {
    pub total_steps: usize,
    pub phases: Vec<PhaseSummary>,
    pub llm_calls: usize,
    pub tool_calls: Vec<ToolCallSummary>,
    pub total_duration_ms: Option<i64>,
    pub issues: Vec<Issue>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhaseSummary {
    pub name: String,
    pub steps: usize,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallSummary {
    pub name: String,
    pub count: usize,
    pub successes: usize,
    pub failures: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    Loop,
    Stall,
    HighErrorRate,
    ExcessiveLlmCalls,
    PhaseRegression,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub kind: IssueKind,
    pub description: String,
    pub step_range: (usize, usize),
}

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts).ok().map(|dt| dt.to_utc())
}

pub fn summarize(log: &ThreadLog) -> ThreadSummary {
    let total_steps = log.steps.len();

    let mut phases: Vec<PhaseSummary> = Vec::new();
    let mut current_phase: Option<(String, usize, Option<DateTime<Utc>>)> = None;

    let mut llm_calls: usize = 0;
    let mut tool_map: HashMap<String, (usize, usize, usize)> = HashMap::new();

    for (idx, step) in log.steps.iter().enumerate() {
        match step {
            ThreadStep::Phase { phase, ts, .. } => {
                if let Some((name, start_idx, start_ts)) = current_phase.take() {
                    let dur = start_ts
                        .zip(parse_ts(ts))
                        .map(|(s, e)| (e - s).num_milliseconds());
                    phases.push(PhaseSummary {
                        name,
                        steps: idx - start_idx,
                        duration_ms: dur,
                    });
                }
                current_phase = Some((phase.clone(), idx, parse_ts(ts)));
            }
            ThreadStep::LlmEnd { status, .. } => {
                if *status == LlmStepStatus::Ok {
                    llm_calls += 1;
                }
            }
            ThreadStep::ToolEnd { name, status, .. } => {
                let entry = tool_map.entry(name.clone()).or_insert((0, 0, 0));
                entry.0 += 1;
                match status {
                    ToolStepStatus::Ok => entry.1 += 1,
                    ToolStepStatus::Failed => entry.2 += 1,
                    ToolStepStatus::Running => {}
                }
            }
            _ => {}
        }
    }

    if let Some((name, start_idx, start_ts)) = current_phase.take() {
        let last_ts = log.steps.last().and_then(|s| parse_ts(s.ts()));
        let dur = start_ts
            .zip(last_ts)
            .map(|(s, e)| (e - s).num_milliseconds());
        phases.push(PhaseSummary {
            name,
            steps: total_steps.saturating_sub(start_idx),
            duration_ms: dur,
        });
    }

    let tool_calls: Vec<ToolCallSummary> = {
        let mut v: Vec<_> = tool_map
            .into_iter()
            .map(|(name, (count, ok, fail))| ToolCallSummary {
                name,
                count,
                successes: ok,
                failures: fail,
            })
            .collect();
        v.sort_by(|a, b| b.count.cmp(&a.count));
        v
    };

    let total_duration_ms = log
        .steps
        .first()
        .and_then(|f| parse_ts(f.ts()))
        .zip(log.steps.last().and_then(|l| parse_ts(l.ts())))
        .map(|(s, e)| (e - s).num_milliseconds());

    let result = log.result.as_ref().map(|r| r.kind.clone());

    let issues = detect_issues(log);

    ThreadSummary {
        total_steps,
        phases,
        llm_calls,
        tool_calls,
        total_duration_ms,
        issues,
        result,
    }
}

pub fn detect_issues(log: &ThreadLog) -> Vec<Issue> {
    let mut issues = Vec::new();
    detect_loops(log, &mut issues);
    detect_stalls(log, &mut issues);
    detect_high_error_rate(log, &mut issues);
    detect_excessive_llm_calls(log, &mut issues);
    detect_phase_regression(log, &mut issues);
    issues
}

fn detect_loops(log: &ThreadLog, issues: &mut Vec<Issue>) {
    let window = 10;
    let threshold = 3;
    let iterations = log.steps.len().saturating_sub(window).max(1).min(log.steps.len());

    for start in 0..iterations {
        let end = (start + window).min(log.steps.len());
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for step in &log.steps[start..end] {
            if let ThreadStep::ToolEnd { name, .. } = step {
                *counts.entry(name.as_str()).or_default() += 1;
            }
        }
        for (name, count) in &counts {
            if *count >= threshold {
                let already = issues.iter().any(|i| {
                    i.kind == IssueKind::Loop
                        && i.description.contains(name)
                        && i.step_range.1 >= start
                });
                if !already {
                    issues.push(Issue {
                        kind: IssueKind::Loop,
                        description: format!(
                            "Tool '{name}' called {count} times in {window} steps"
                        ),
                        step_range: (start, end),
                    });
                }
            }
        }
    }
}

fn detect_stalls(log: &ThreadLog, issues: &mut Vec<Issue>) {
    let threshold_ms: i64 = 60_000;
    for i in 1..log.steps.len() {
        let prev_ts = parse_ts(log.steps[i - 1].ts());
        let cur_ts = parse_ts(log.steps[i].ts());
        if let (Some(p), Some(c)) = (prev_ts, cur_ts) {
            let gap = (c - p).num_milliseconds();
            if gap > threshold_ms {
                issues.push(Issue {
                    kind: IssueKind::Stall,
                    description: format!("Gap of {:.1}s between steps", gap as f64 / 1000.0),
                    step_range: (i - 1, i),
                });
            }
        }
    }
}

fn detect_high_error_rate(log: &ThreadLog, issues: &mut Vec<Issue>) {
    let mut current_phase = String::new();
    let mut phase_start = 0;
    let mut ok_count: usize = 0;
    let mut fail_count: usize = 0;

    let flush = |phase: &str,
                 start: usize,
                 end: usize,
                 ok: usize,
                 fail: usize,
                 issues: &mut Vec<Issue>| {
        let total = ok + fail;
        if total >= 4 && fail as f64 / total as f64 > 0.5 {
            issues.push(Issue {
                kind: IssueKind::HighErrorRate,
                description: format!(
                    "Phase '{phase}': {fail}/{total} tool calls failed"
                ),
                step_range: (start, end),
            });
        }
    };

    for (idx, step) in log.steps.iter().enumerate() {
        if let ThreadStep::Phase { phase, .. } = step {
            flush(
                &current_phase,
                phase_start,
                idx,
                ok_count,
                fail_count,
                issues,
            );
            current_phase = phase.clone();
            phase_start = idx;
            ok_count = 0;
            fail_count = 0;
        }
        if let ThreadStep::ToolEnd { status, .. } = step {
            match status {
                ToolStepStatus::Ok => ok_count += 1,
                ToolStepStatus::Failed => fail_count += 1,
                ToolStepStatus::Running => {}
            }
        }
    }
    flush(
        &current_phase,
        phase_start,
        log.steps.len(),
        ok_count,
        fail_count,
        issues,
    );
}

fn detect_excessive_llm_calls(log: &ThreadLog, issues: &mut Vec<Issue>) {
    let threshold = 10;
    let mut consecutive_llm = 0;
    let mut run_start = 0;

    for (idx, step) in log.steps.iter().enumerate() {
        match step {
            ThreadStep::LlmEnd { .. } => {
                if consecutive_llm == 0 {
                    run_start = idx;
                }
                consecutive_llm += 1;
            }
            ThreadStep::ToolEnd {
                status: ToolStepStatus::Ok,
                ..
            } => {
                consecutive_llm = 0;
            }
            _ => {}
        }
        if consecutive_llm >= threshold {
            issues.push(Issue {
                kind: IssueKind::ExcessiveLlmCalls,
                description: format!(
                    "{consecutive_llm} LLM calls without a successful tool call"
                ),
                step_range: (run_start, idx),
            });
            consecutive_llm = 0;
        }
    }
}

fn detect_phase_regression(log: &ThreadLog, issues: &mut Vec<Issue>) {
    let mut phase_order: Vec<String> = Vec::new();
    for step in &log.steps {
        if let ThreadStep::Phase { phase, .. } = step {
            if phase_order.last() != Some(phase) {
                phase_order.push(phase.clone());
            }
        }
    }

    for i in 1..phase_order.len() {
        let cur = &phase_order[i];
        if let Some(first_seen) = phase_order[..i].iter().position(|p| p == cur) {
            if first_seen < i.saturating_sub(1) {
                let step_idx = log
                    .steps
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| matches!(s, ThreadStep::Phase { phase, .. } if phase == cur))
                    .nth(1)
                    .map_or(0, |(idx, _)| idx);
                issues.push(Issue {
                    kind: IssueKind::PhaseRegression,
                    description: format!(
                        "Phase '{cur}' revisited (first at position {first_seen}, again at {i})"
                    ),
                    step_range: (step_idx, step_idx),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Observation, ThreadLog, ThreadStep, THREAD_SCHEMA_VERSION};

    fn make_log(steps: Vec<ThreadStep>) -> ThreadLog {
        ThreadLog {
            schema_version: THREAD_SCHEMA_VERSION,
            steps,
            result: None,
            title: None,
            title_locked: false,
        }
    }

    #[test]
    fn summarize_empty_log() {
        let log = make_log(vec![]);
        let s = summarize(&log);
        assert_eq!(s.total_steps, 0);
        assert!(s.phases.is_empty());
        assert!(s.issues.is_empty());
    }

    #[test]
    fn detect_loop_issue() {
        let mut steps = Vec::new();
        for _ in 0..4 {
            steps.push(ThreadStep::ToolEnd {
                tool_id: "1".into(),
                name: "sql_run".into(),
                clean_name: String::new(),
                args: serde_json::Value::Null,
                status: ToolStepStatus::Ok,
                payload: None,
                ctx: None,
                observation: crate::session::ToolObservation::normalize(
                    serde_json::json!({"ok": true}),
                ),
                ts: "2026-03-27T10:00:00Z".into(),
                agent: "test".into(),
            });
        }
        let log = make_log(steps);
        let issues = detect_issues(&log);
        assert!(issues.iter().any(|i| i.kind == IssueKind::Loop));
    }

    #[test]
    fn detect_stall_issue() {
        let steps = vec![
            ThreadStep::Phase {
                phase: "plan".into(),
                from_phase: None,
                reason_code: None,
                reason_detail: None,
                observation: Observation::ok(),
                ts: "2026-03-27T10:00:00Z".into(),
                agent: "test".into(),
            },
            ThreadStep::Phase {
                phase: "author".into(),
                from_phase: Some("plan".into()),
                reason_code: None,
                reason_detail: None,
                observation: Observation::ok(),
                ts: "2026-03-27T10:02:00Z".into(),
                agent: "test".into(),
            },
        ];
        let log = make_log(steps);
        let issues = detect_issues(&log);
        assert!(issues.iter().any(|i| i.kind == IssueKind::Stall));
    }
}
