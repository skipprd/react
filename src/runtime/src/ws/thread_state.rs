use super::mapping::{map_exec_ctx, map_thread_event_kind, map_tool_event_status};
use super::util::{env_truthy, truncate_str};
use crate::ws::api_gen::src::models as api;
use react_core::session::{
    Observation, ThreadItemError as CoreThreadItemError, ThreadItemKind as CoreThreadItemKind,
    ThreadItemState as CoreThreadItemState, ThreadItemStatus as CoreThreadItemStatus,
    ThreadState as CoreThreadState, ThreadStep, ThreadStore,
};
use react_core::suite::SuiteRegistry;
use std::collections::BTreeMap;

pub(super) fn ws_thread_state_snapshot_from_core(
    st: &CoreThreadState,
    timeline_events: &[react_core::session::ThreadEvent],
    reg: &SuiteRegistry,
) -> api::ThreadStateSnapshot {
    fn elapsed_ms_since(start_ts: &str) -> Option<i64> {
        let start = chrono::DateTime::parse_from_rfc3339(start_ts).ok()?;
        let now = chrono::Utc::now();
        let delta = now.signed_duration_since(start.with_timezone(&chrono::Utc));
        let ms = delta.num_milliseconds();
        Some(ms.max(0))
    }

    let mut items: Vec<api::ThreadStateItem> = Vec::new();
    for (item_id, it) in st.items.iter() {
        let mut wi = api::ThreadStateItem::new(
            item_id.clone(),
            it.kind.as_str().to_string(),
            it.status.as_str().to_string(),
        );
        wi.started_at = it.started_at.clone();
        wi.finished_at = it.finished_at.clone();
        wi.runtime_ms = it.runtime_ms.map(|n| n as i64);
        if wi.runtime_ms.is_none() {
            if let (Some(ref started), None) = (&wi.started_at, &wi.finished_at) {
                wi.runtime_ms = elapsed_ms_since(started);
            }
        }
        wi.outputs = it.outputs.as_ref().and_then(|v| v.as_object()).map(|obj| {
            let mut out: std::collections::HashMap<String, serde_json::Value> =
                std::collections::HashMap::new();
            for (k, vv) in obj.iter() {
                out.insert(k.clone(), vv.clone());
            }
            out
        });
        if let Some(ref e) = it.last_error {
            let mut we = api::ThreadStateItemError::new(e.summary.clone());
            we.tool_step_idx = e.tool_step_idx.map(|n| n as i32);
            we.step_ts = e.step_ts.clone();
            wi.last_error = Some(we);
        }
        items.push(wi);
    }
    items.sort_by(|a, b| a.item_id.cmp(&b.item_id));

    let mut total_runtime_ms: i64 = st.total_runtime_ms as i64;
    if let Some(ref cur) = st.current_phase {
        let cur_id = format!("phase:{}", cur);
        if let Some(cur_item) = st.items.get(&cur_id) {
            if cur_item.finished_at.is_none() {
                if let Some(ref started) = cur_item.started_at {
                    total_runtime_ms += elapsed_ms_since(started).unwrap_or(0);
                }
            }
        }
    }

    let suite_id = st.suite_id.as_deref().unwrap_or("");
    let agent_type = st.agent_type.as_deref().unwrap_or("ask");
    let phases: Vec<String> = reg
        .get(suite_id)
        .map(|s| s.phase_order(agent_type))
        .unwrap_or_default();
    let current_phase = st
        .current_phase
        .clone()
        .unwrap_or_else(|| "preflight".to_string());
    let completed_phases = derive_completed_phases(&phases, &current_phase, &st.items);

    let events: Vec<api::ThreadEvent> = timeline_events
        .iter()
        .map(|ev| {
            let kind = map_thread_event_kind(ev.event_kind);
            let mut out = api::ThreadEvent::new(ev.step_idx as i32, kind, ev.ts.clone());
            out.tool_id = ev.tool_id.clone();
            out.name = ev.name.clone();
            out.clean_name = ev.clean_name.clone();
            out.runtime_ms = ev.runtime_ms.map(|n| n as i64);
            out.error = ev.error.clone();
            out.call_id = ev.call_id.map(|n| n as i32);
            out.model = ev.model.clone();
            out.phase = ev.phase.clone();
            out.status = ev.status.map(|s| map_tool_event_status(Some(s)));
            out.payload = ev.payload.as_ref().and_then(|v| v.as_object()).map(|obj| {
                let mut hm: std::collections::HashMap<String, serde_json::Value> =
                    std::collections::HashMap::new();
                for (k, vv) in obj.iter() {
                    hm.insert(k.clone(), vv.clone());
                }
                hm
            });
            out.ctx = ev.ctx.as_ref().map(map_exec_ctx);
            out
        })
        .collect();

    let mut snap = api::ThreadStateSnapshot::new(
        st.thread_state_schema_version as i32,
        st.thread_id.clone(),
        st.last_materialized_step_count as i32,
        total_runtime_ms.max(0),
        phases,
        completed_phases,
        events,
        items,
    );
    snap.suite_id = st.suite_id.clone();
    snap.agent_type = st.agent_type.clone();
    snap.current_phase = st.current_phase.clone();
    if let Some(suite_state) = st.suite_state.as_ref().and_then(|v| v.as_object()) {
        let mut hm: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
        for (k, v) in suite_state.iter() {
            hm.insert(k.clone(), v.clone());
        }
        if !hm.is_empty() {
            snap.plan_summaries = Some(hm);
        }
    }
    snap
}

pub(super) async fn load_timeline_events(store: &ThreadStore, thread_id: &str) -> Vec<react_core::session::ThreadEvent> {
    store
        .get_thread_events_from_log(thread_id)
        .await
        .unwrap_or_default()
}

pub(super) async fn upsert_thread_state_from_plans(
    store: &ThreadStore,
    thread_id: &str,
    plans: &[api::PlanSnapshot],
) {
    let mut st = match store.get_thread_state(thread_id).await {
        Ok(s) => s,
        Err(_) => return,
    };

    fn summarize_plan(p: &api::PlanSnapshot) -> serde_json::Value {
        let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for t in p.tasks.iter() {
            let st = t.status;
            let k = format!("{:?}", st).to_lowercase();
            *counts.entry(k).or_insert(0) += 1;
        }
        serde_json::json!({
            "planKey": p.plan_key,
            "status": format!("{:?}", p.status).to_lowercase(),
            "taskCounts": counts,
        })
    }

    fn map_task_status_to_item_status(s: api::PlanTaskStatus) -> CoreThreadItemStatus {
        match s {
            api::PlanTaskStatus::Pending => CoreThreadItemStatus::Queued,
            api::PlanTaskStatus::InProgress => CoreThreadItemStatus::Running,
            api::PlanTaskStatus::Done => CoreThreadItemStatus::Ok,
            api::PlanTaskStatus::Blocked => CoreThreadItemStatus::Blocked,
            api::PlanTaskStatus::NeedsUpdate => CoreThreadItemStatus::Blocked,
        }
    }

    fn task_error_from_checklist(items: &[api::PlanChecklistItem]) -> Option<String> {
        let mut pick: Option<&api::PlanChecklistItem> = None;
        for it in items.iter() {
            match it.status {
                api::PlanChecklistItemStatus::NeedsUpdate => {
                    pick = Some(it);
                    break;
                }
                api::PlanChecklistItemStatus::Blocked => {
                    if pick.is_none() {
                        pick = Some(it);
                    }
                }
                _ => {}
            }
        }
        let Some(it) = pick else { return None };
        let details = it.details.as_deref().unwrap_or("").trim();
        if details.is_empty() {
            Some(it.label.clone())
        } else {
            Some(format!("{}: {}", it.label, details))
        }
    }

    for p in plans.iter() {
        let kind = if p.plan_kind.trim().is_empty() {
            "plan".to_string()
        } else {
            p.plan_kind.clone()
        };
        let suite_state = st
            .suite_state
            .get_or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = suite_state.as_object_mut() {
            obj.insert(kind, summarize_plan(p));
        }

        for t in p.tasks.iter() {
            let task_id = t.task_id.clone();
            let status = t.status;
            let checklist = t.checklist.clone();
            let outputs = serde_json::json!({
                "task_kind": t.task_kind,
                "label": t.label,
                "details": t.details,
            });
            let item_id = format!("task:plan:{}:{}", p.plan_key, task_id);
            let ent = st
                .items
                .entry(item_id)
                .or_insert_with(|| CoreThreadItemState {
                    kind: CoreThreadItemKind::Task,
                    status: CoreThreadItemStatus::Queued,
                    started_at: None,
                    finished_at: None,
                    runtime_ms: None,
                    last_error: None,
                    outputs: None,
                });
            ent.kind = CoreThreadItemKind::Task;
            ent.status = map_task_status_to_item_status(status);
            ent.outputs = Some(outputs);
            if let Some(err) = task_error_from_checklist(&checklist) {
                ent.last_error = Some(CoreThreadItemError {
                    summary: err,
                    tool_step_idx: None,
                    step_ts: None,
                });
            }
        }
    }

    let _ = store.put_thread_state(thread_id, &st).await;
}

pub(super) async fn append_step_if_new(store: &ThreadStore, thread_id: &str, step: ThreadStep) {
    let is_dup = match store
        .get(thread_id)
        .await
        .ok()
        .and_then(|l| l.steps.last().cloned())
    {
        Some(ThreadStep::Interrupt { prompt: p1, .. }) => {
            matches!(&step, ThreadStep::Interrupt { prompt: p2, .. } if p1 == *p2)
        }
        Some(ThreadStep::Complete {
            kind: k1,
            payload: pl1,
            display: d1,
            ..
        }) => matches!(
            &step,
            ThreadStep::Complete { kind: k2, payload: pl2, display: d2, .. }
                if k1 == *k2 && pl1 == *pl2 && d1 == *d2
        ),
        _ => false,
    };
    if is_dup {
        return;
    }
    let _ = store.append_step(thread_id, step).await;
}

pub(super) fn summarize_step(step: &ThreadStep) -> String {
    let agent = match step {
        ThreadStep::SwitchSuite { agent, .. }
        | ThreadStep::SwitchAgent { agent, .. }
        | ThreadStep::User { agent, .. }
        | ThreadStep::ToolStart { agent, .. }
        | ThreadStep::ToolEnd { agent, .. }
        | ThreadStep::LlmStart { agent, .. }
        | ThreadStep::LlmEnd { agent, .. }
        | ThreadStep::LlmCall { agent, .. }
        | ThreadStep::Phase { agent, .. }
        | ThreadStep::GuardBlock { agent, .. }
        | ThreadStep::ArtifactFocus { agent, .. }
        | ThreadStep::ArtifactSaved { agent, .. }
        | ThreadStep::Interrupt { agent, .. }
        | ThreadStep::ReviewResponse { agent, .. }
        | ThreadStep::Complete { agent, .. }
        | ThreadStep::Checkpoint { agent, .. } => agent.as_str(),
    };
    let raw = serde_json::to_string(step).unwrap_or_else(|_| "{\"type\":\"unknown\"}".to_string());
    let raw = truncate_str(&raw, 220);
    format!("ts={} agent={} step={}", step.ts(), agent, raw)
}

pub(super) async fn ensure_preflight_phase_step(
    store: &ThreadStore,
    thread_id: &str,
    agent: &str,
    suite_id: Option<&str>,
) -> Result<Option<(usize, String)>, String> {
    let log = store.get(thread_id).await?;
    let already_has_phase = log
        .steps
        .iter()
        .any(|s| matches!(s, ThreadStep::Phase { .. }));
    if already_has_phase {
        return Ok(None);
    }
    let step_idx = log.steps.len();
    let ts = chrono::Utc::now().to_rfc3339();
    let mut detail = serde_json::Map::new();
    detail.insert("phase".to_string(), serde_json::json!("preflight"));
    if let Some(s) = suite_id {
        if !s.trim().is_empty() {
            detail.insert("suite_id".to_string(), serde_json::json!(s));
        }
    }
    if !agent.trim().is_empty() {
        detail.insert("agent".to_string(), serde_json::json!(agent));
    }
    let _ = store
        .append_step(
            thread_id,
            ThreadStep::Phase {
                phase: "preflight".to_string(),
                from_phase: None,
                reason_code: Some("preflight_start".to_string()),
                reason_detail: Some(serde_json::Value::Object(detail)),
                observation: Observation::ok(),
                ts: ts.clone(),
                agent: agent.to_string(),
            },
        )
        .await;
    Ok(Some((step_idx, ts)))
}

pub(super) fn duration_ms(start_ts: &str, end_ts: &str) -> Option<i64> {
    let start = chrono::DateTime::parse_from_rfc3339(start_ts).ok()?;
    let end = chrono::DateTime::parse_from_rfc3339(end_ts).ok()?;
    let delta = end.signed_duration_since(start);
    Some(delta.num_milliseconds().max(0))
}

pub(super) fn phase_runs_from_steps(
    steps: &[ThreadStep],
) -> std::collections::HashMap<String, Vec<api::PhaseRun>> {
    let mut runs: std::collections::HashMap<String, Vec<api::PhaseRun>> =
        std::collections::HashMap::new();
    for step in steps.iter() {
        let ThreadStep::Phase {
            phase,
            from_phase,
            ts,
            ..
        } = step
        else {
            continue;
        };
        let ts = ts.clone();

        if let Some(prev) = from_phase
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            if let Some(v) = runs.get_mut(prev) {
                if let Some(last) = v.last_mut() {
                    if last.ended_at.is_none() {
                        last.ended_at = Some(ts.clone());
                        last.runtime_ms = duration_ms(&last.started_at, &ts);
                    }
                }
            }
        }

        let ph = phase.trim();
        if ph.is_empty() {
            continue;
        }
        let v = runs.entry(ph.to_string()).or_default();
        if let Some(last) = v.last_mut() {
            if last.ended_at.is_none() {
                last.ended_at = Some(ts.clone());
                last.runtime_ms = duration_ms(&last.started_at, &ts);
            }
        }
        v.push(api::PhaseRun::new(ts));
    }
    runs
}

pub(super) fn total_completed_runtime_ms(runs: &[api::PhaseRun]) -> i64 {
    runs.iter().filter_map(|r| r.runtime_ms).sum()
}

pub(super) fn derive_current_phase_from_steps(steps: &[ThreadStep]) -> String {
    for step in steps.iter().rev() {
        if let ThreadStep::Phase { phase, .. } = step {
            let t = phase.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    "preflight".to_string()
}

pub(super) fn phase_at_step_idx(steps: &[ThreadStep], idx: usize) -> Option<String> {
    if steps.is_empty() {
        return None;
    }
    let end = idx.min(steps.len().saturating_sub(1));
    Some(derive_current_phase_from_steps(&steps[..=end]))
}

pub(super) fn derive_completed_phases(
    order: &[String],
    current: &str,
    items: &BTreeMap<String, CoreThreadItemState>,
) -> Vec<String> {
    let mut completed: Vec<String> = Vec::new();
    if order.is_empty() {
        return completed;
    }
    let mut idx: Option<usize> = None;
    for (i, p) in order.iter().enumerate() {
        if p == current {
            idx = Some(i);
            break;
        }
    }
    let upto = idx.unwrap_or(0);
    for p in order.iter().take(upto) {
        completed.push(p.clone());
    }
    for (key, it) in items.iter() {
        if let Some(phase_name) = key.strip_prefix("phase:") {
            if it.status == CoreThreadItemStatus::Ok && !completed.iter().any(|p| p == phase_name)
            {
                completed.push(phase_name.to_string());
            }
        }
    }
    completed
}

pub(super) async fn log_thread_steps_if_enabled(store: &ThreadStore, thread_id: &str, reason: &str) {
    if !env_truthy("REACT_LOG_THREAD_STEPS") {
        return;
    }
    match store.get(thread_id).await {
        Ok(log) => {
            tracing::info!(
                "THREAD_LOG {} thread_id={} steps={} title={:?} finalized={}",
                reason,
                thread_id,
                log.steps.len(),
                log.title,
                log.title_locked
            );
            let n = 40usize;
            let start = log.steps.len().saturating_sub(n);
            for (i, step) in log.steps.iter().enumerate().skip(start) {
                tracing::info!(
                    "THREAD_STEP {} #{} {}",
                    thread_id,
                    i + 1,
                    summarize_step(step)
                );
            }
            if env_truthy("REACT_LOG_THREAD_JSON") {
                if let Ok(pretty) = serde_json::to_string_pretty(&log) {
                    tracing::info!("THREAD_JSON {} {}", thread_id, pretty);
                }
            }
        }
        Err(e) => tracing::info!(
            "THREAD_LOG {} thread_id={} (failed to load: {})",
            reason,
            thread_id,
            e
        ),
    }
}
