//! Headless runner for app-owned hosts.
//!
//! Design goal: subscribe to the same typed events as WS without using a socket.

use react_core::session::{ControlStateStore, ThreadEvent, ThreadEventStatus, ThreadLogReader};
use react_core::suite::{SuiteCtx, SuiteRegistry};
use react_view::{ThreadItemStatus, ThreadLogViewCache};
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use crate::event_hub::EventHub;
use crate::ws::api_gen::src::models as api;

const DEFAULT_EVENT_HUB_CAPACITY: usize = 4096;

fn classify_error(summary: &str) -> &'static str {
    let s = summary.to_ascii_lowercase();
    if s.contains("llm_error")
        || s.contains("an error occurred while processing your request")
        || s.contains("overloaded")
    {
        "llm_transient"
    } else if s.contains("timed out reading response")
        || s.contains("timed out")
        || s.contains("network error")
    {
        "network_timeout"
    } else if s.contains("max_output_tokens") || s.contains("output truncated") {
        "token_truncation"
    } else if s.contains("too many consecutive batch failures") || s.contains("batch_locked") {
        "batch_locked"
    } else if s.contains("invalid staging model sql") {
        "invalid_staging_sql"
    } else if s.contains("nosuchkey") || s.contains("no such key") || s.contains("not found") {
        "missing_artifact"
    } else {
        "unknown"
    }
}

fn summarize_failure_state(st: &ThreadLogViewCache, events: &[ThreadEvent]) -> Option<String> {
    let last_failed_event = events
        .iter()
        .rev()
        .find(|e| e.status == Some(ThreadEventStatus::Failed));
    let mut detail = String::new();
    let mut phase = None::<String>;
    let mut tool = None::<String>;
    if let Some(ev) = last_failed_event {
        phase = ev.phase.clone();
        tool = ev.clean_name.clone().or_else(|| ev.name.clone());
        if let Some(err) = ev
            .error
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            detail = err.to_string();
        }
    }
    if detail.is_empty() {
        if let Some(it) = st
            .items
            .values()
            .find(|it| it.status == ThreadItemStatus::Failed)
        {
            if let Some(err) = it
                .last_error
                .as_ref()
                .map(|e| e.summary.trim())
                .filter(|s| !s.is_empty())
            {
                detail = err.to_string();
            }
        }
    }
    if detail.is_empty() {
        return None;
    }
    let class = classify_error(&detail);
    let mut out = format!("failure summary: class={}", class);
    if let Some(p) = phase.filter(|p| !p.trim().is_empty()) {
        out.push_str(&format!(" phase={}", p));
    }
    if let Some(t) = tool.filter(|t| !t.trim().is_empty()) {
        out.push_str(&format!(" tool={}", t));
    }
    out.push_str(&format!(" detail={}", detail.replace('\n', " | ")));
    Some(out)
}

#[derive(Clone, Debug)]
pub struct RunOpts {
    pub thread_id: Option<String>,
    pub suite_id: String,
    pub agent: String, // default: agent
    /// Optional channel to publish the thread_id as soon as it is observed.
    pub thread_id_tx: Option<mpsc::UnboundedSender<String>>,
}

/// Result returned by [`run_headless`].
#[derive(Clone, Debug)]
pub struct HeadlessResult {
    pub exit_code: i32,
    pub thread_id: String,
    /// Human-readable failure summary when exit_code != 0.
    pub failure_summary: Option<String>,
}

/// Run a headless thread execution and return the final exit code.
///
/// Exit codes:
/// - 0: no failed items in materialized thread_state
/// - 1: at least one failed item
/// - 2: no final state could be determined
pub async fn run_headless(
    ctx: SuiteCtx,
    opts: RunOpts,
    registry: SuiteRegistry,
) -> Result<HeadlessResult, String> {
    let hub = EventHub::new(DEFAULT_EVENT_HUB_CAPACITY);
    let mut rx = hub.subscribe();
    let plain_progress = std::env::var("REACT_PLAIN_PROGRESS")
        .ok()
        .filter(|v| !v.trim().is_empty() && v != "0" && v.to_ascii_lowercase() != "false")
        .is_some();

    // Capture the thread id from events (for `new`) and detect completion.
    let mut tid_tx = opts.thread_id_tx.clone();
    let capture = tokio::spawn(async move {
        let mut last_thread_id: Option<String> = None;
        let mut saw_final: bool = false;
        let mut sent_tid: bool = false;
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    match msg {
                        api::ServerMessage::Phase(r) => {
                            if plain_progress {
                                let from = r.from_phase.unwrap_or_else(|| "-".to_string());
                                if from == r.phase {
                                    let reason = r
                                        .reason_code
                                        .map(|x| format!("{:?}", x))
                                        .unwrap_or_else(|| "checkpoint".to_string());
                                    println!(
                                        "phase checkpoint {} [{}] ({})",
                                        r.phase, reason, r.ts
                                    );
                                } else {
                                    println!("phase {} -> {} ({})", from, r.phase, r.ts);
                                }
                            }
                        }
                        api::ServerMessage::ToolStart(r) => {
                            if plain_progress {
                                let label = r.clean_name.unwrap_or(r.name);
                                println!("tool start: {}", label);
                            }
                        }
                        api::ServerMessage::ToolEnd(r) => {
                            if plain_progress {
                                let label = r.clean_name.unwrap_or(r.name);
                                if matches!(r.status, api::ToolEventStatus::Failed) {
                                    let err = r
                                        .error
                                        .as_deref()
                                        .map(|s| s.trim())
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or("tool failed");
                                    // Keep this single-line and CI-friendly.
                                    println!("tool end: {} (Failed) — {}", label, err);
                                } else {
                                    println!("tool end: {} ({:?})", label, r.status);
                                }
                            }
                        }
                        api::ServerMessage::Final(r) => {
                            last_thread_id = Some(r.thread_id.clone());
                            if let Some(tx) = tid_tx.take() {
                                let _ = tx.send(r.thread_id.clone());
                            }
                            if plain_progress {
                                println!("final thread: {}", r.thread_id);
                            }
                            saw_final = true;
                            break;
                        }
                        api::ServerMessage::AwaitUser(r) => {
                            last_thread_id = Some(r.thread_id.clone());
                            if !sent_tid {
                                if let Some(tx) = tid_tx.take() {
                                    let _ = tx.send(r.thread_id.clone());
                                }
                                sent_tid = true;
                            }
                        }
                        api::ServerMessage::AwaitApproval(r) => {
                            last_thread_id = Some(r.thread_id.clone());
                            if !sent_tid {
                                if let Some(tx) = tid_tx.take() {
                                    let _ = tx.send(r.thread_id.clone());
                                }
                                sent_tid = true;
                            }
                        }
                        api::ServerMessage::ThreadState(r) => {
                            last_thread_id = Some(r.thread_id.clone());
                            if !sent_tid {
                                if let Some(tx) = tid_tx.take() {
                                    let _ = tx.send(r.thread_id.clone());
                                }
                                sent_tid = true;
                            }
                        }
                        _ => {}
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
        (last_thread_id, saw_final)
    });

    let requested_tid = opts.thread_id.clone();
    let suite_id = opts.suite_id.clone();
    let tid = crate::ws::server::run_headless_with_hub(
        ctx.clone(),
        opts.thread_id,
        opts.suite_id,
        opts.agent,
        hub.clone(),
        registry,
    )
    .await?;

    // Closing the hub allows the capture task to finish even if final wasn't observed.
    drop(hub);
    let (captured_tid, saw_final) = capture
        .await
        .map_err(|e| format!("internal: capture task failed: {e}"))?;

    let thread_id = requested_tid
        .or_else(|| captured_tid)
        .unwrap_or_else(|| tid.clone());

    // Determine exit code: prefer ControlState, fall back to view cache.
    let reader = ctx.log_reader();
    let control = ControlStateStore::new(
        ctx.storage().clone(),
        ctx.scope().clone(),
        ctx.keyspace().clone(),
    );

    // Check ControlState for execution outcome (primary source of truth).
    if let Ok(Some(outcome)) = control
        .load::<serde_json::Value>(&thread_id, &suite_id)
        .await
    {
        if let Some(mode) = outcome
            .get("phase_state")
            .and_then(|ps| ps.get("mode"))
            .and_then(|m| m.as_str())
        {
            if mode == "failed" {
                let summary = match reader.get_log(&thread_id).await {
                    Ok(log) => {
                        let st =
                            crate::ws::thread_state::materialize_state_from_log(&thread_id, &log);
                        let events = react_view::build_thread_events_from_log(&log, 500);
                        summarize_failure_state(&st, &events)
                    }
                    Err(_) => None,
                };
                if plain_progress {
                    if let Some(ref line) = summary {
                        println!("{}", line);
                    }
                }
                return Ok(HeadlessResult {
                    exit_code: 1,
                    thread_id,
                    failure_summary: summary,
                });
            }
            return Ok(HeadlessResult {
                exit_code: 0,
                thread_id,
                failure_summary: None,
            });
        }
    }

    if !saw_final {
        return Ok(HeadlessResult {
            exit_code: 2,
            thread_id,
            failure_summary: Some("Thread did not reach a final state.".to_string()),
        });
    }
    Ok(HeadlessResult {
        exit_code: 0,
        thread_id,
        failure_summary: None,
    })
}
