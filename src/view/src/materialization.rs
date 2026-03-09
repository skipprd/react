use crate::{ThreadItemError, ThreadItemKind, ThreadItemState, ThreadItemStatus, ThreadLogViewCache};
use react_core::session::ThreadStep;

fn duration_ms(start_ts: &str, end_ts: &str) -> Option<u64> {
    let start = chrono::DateTime::parse_from_rfc3339(start_ts).ok()?;
    let end = chrono::DateTime::parse_from_rfc3339(end_ts).ok()?;
    let delta = end.signed_duration_since(start);
    let ms = delta.num_milliseconds();
    if ms <= 0 {
        return Some(0);
    }
    Some(ms as u64)
}

pub fn apply_step_to_state(st: &mut ThreadLogViewCache, _step_idx: usize, step: &ThreadStep) {
    match step {
        ThreadStep::SwitchSuite { to, .. } => {
            if !to.trim().is_empty() {
                st.suite_id = Some(to.to_string());
            }
        }
        ThreadStep::SwitchAgent { to, .. } => {
            if !to.trim().is_empty() {
                st.agent_type = Some(to.to_string());
            }
        }
        ThreadStep::Phase {
            phase,
            from_phase,
            ts,
            ..
        } => {
            let ph = phase.trim().to_string();
            if ph.is_empty() {
                return;
            }

            if let Some(prev) = from_phase
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                let key = format!("phase:{}", prev);
                let ent = st
                    .items
                    .entry(key)
                    .or_insert_with(|| ThreadItemState::phase_finished(ts));
                ent.kind = ThreadItemKind::Phase;
                ent.status = ThreadItemStatus::Ok;
                ent.finished_at.get_or_insert_with(|| ts.clone());
                if ent.runtime_ms.is_none() {
                    if let (Some(started), Some(finished)) = (&ent.started_at, &ent.finished_at) {
                        ent.runtime_ms = duration_ms(started, finished);
                    }
                }
            }

            let key = format!("phase:{}", ph);
            let ent = st
                .items
                .entry(key)
                .or_insert_with(|| ThreadItemState::phase_running(ts));
            ent.kind = ThreadItemKind::Phase;
            ent.status = ThreadItemStatus::Running;
            ent.started_at.get_or_insert_with(|| ts.clone());
            st.current_phase = Some(ph);
        }
        ThreadStep::ToolStart { .. } => {}
        ThreadStep::ToolEnd { .. } => {}
        ThreadStep::LlmStart { .. } => {}
        ThreadStep::LlmEnd { .. } => {}
        ThreadStep::Complete {
            ts, observation, ..
        } => {
            let Some(ph) = st
                .current_phase
                .clone()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            else {
                return;
            };

            let key = format!("phase:{}", ph);
            let ent = st
                .items
                .entry(key)
                .or_insert_with(|| ThreadItemState::phase_running(ts));
            ent.kind = ThreadItemKind::Phase;
            ent.status = if observation.ok {
                ThreadItemStatus::Ok
            } else {
                ThreadItemStatus::Failed
            };
            ent.finished_at = Some(ts.clone());
            if ent.runtime_ms.is_none() {
                if let (Some(started), Some(finished)) = (&ent.started_at, &ent.finished_at) {
                    ent.runtime_ms = duration_ms(started, finished);
                }
            }
        }
        ThreadStep::Checkpoint { display, ts, .. } => {
            if let Some(ph) = &st.current_phase {
                let key = format!("phase:{}", ph);
                let ent = st
                    .items
                    .entry(key)
                    .or_insert_with(|| ThreadItemState::phase_running(ts));
                if let Some(d) = display {
                    ent.last_error = Some(ThreadItemError {
                        summary: d.clone(),
                        tool_step_idx: None,
                        step_ts: Some(ts.clone()),
                    });
                }
            }
        }
        ThreadStep::Interrupt { prompt, ts, .. } => {
            block_current_phase(st, prompt, ts);
        }
        ThreadStep::GuardBlock { reason, ts, .. } => {
            block_current_phase(st, reason, ts);
        }
        ThreadStep::RunLoopStop { reason, ts, .. } => {
            block_current_phase(st, reason, ts);
        }
        ThreadStep::User { .. } => {}
        ThreadStep::LlmCall { .. } => {}
        ThreadStep::ArtifactFocus { .. } => {}
        ThreadStep::ArtifactSaved { .. } => {}
        ThreadStep::ReviewResponse { .. } => {}
    }
}

fn block_current_phase(st: &mut ThreadLogViewCache, msg: &str, ts: &str) {
    let Some(ph) = st.current_phase.clone() else {
        return;
    };
    let key = format!("phase:{}", ph);
    let ent = st
        .items
        .entry(key)
        .or_insert_with(|| ThreadItemState::phase_blocked(ts));
    ent.kind = ThreadItemKind::Phase;
    ent.status = ThreadItemStatus::Blocked;
    ent.last_error = Some(ThreadItemError {
        summary: msg.to_string(),
        tool_step_idx: None,
        step_ts: Some(ts.to_string()),
    });
}
