use super::conn_state::{ConnState, load_latest_plans};
use super::mapping::{map_exec_ctx, ws_final_result_from_typed_final};
use super::thread_state::{
    load_timeline_events, log_thread_steps_if_enabled,
    phase_at_step_idx, phase_runs_from_steps,
    total_completed_runtime_ms, ws_thread_state_snapshot_from_core,
};
use super::util::{env_bool, now_iso, truncate_str, ws_log_out};
use crate::models as m;
use crate::ws::api_gen::src::models as api;
use crate::ws::terminal::TerminalEvent;
use futures_util::SinkExt;
use react_core::interrupt::{InterruptDecision, InterruptPolicy};
use react_core::session::{Observation, ThreadStep, ToolStepStatus};
use std::collections::HashMap;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;

pub(super) enum AgentFrame {
    /// Terminal agent output. Maps to core's `FlowFrame::Complete` / `ThreadStep::Complete`.
    /// The WS API serialises this as `FinalResponse` — the two names are synonymous.
    Final {
        kind: String,
        payload: serde_json::Value,
        display: Option<String>,
    },
    Review {
        text: String,
        meta: Option<serde_json::Value>,
    },
    AwaitUser {
        prompt: String,
    },
    AwaitApproval {
        prompt: String,
    },
}

#[derive(Clone, Copy, Debug)]
pub(super) enum SuiteRunKind {
    New,
    Open,
    User,
}

pub(super) async fn run_suite_and_stream(
    thread_id: &str,
    question: &str,
    suite_id: &str,
    agent: &str,
    cid: &str,
    kind: SuiteRunKind,
    state: &mut ConnState,
    write: &mut (impl SinkExt<Message> + Unpin),
) -> Result<(), String> {
    let policy: Arc<dyn InterruptPolicy> = build_interrupt_policy(cid);
    run_agent_with_processing_suite(
        thread_id, question, suite_id, agent, cid, kind, state, write, &*policy,
    )
    .await
}

fn build_interrupt_policy(cid: &str) -> Arc<dyn InterruptPolicy> {
    let headless = env_bool("REACT_HEADLESS", false) || crate::ws::terminal::enabled();
    if cid == "headless" || headless {
        let auto_approve = env_bool("REACT_HEADLESS_AUTO_APPROVE", true);
        if auto_approve {
            Arc::new(react_core::interrupt::AutoApprovePolicy)
        } else {
            Arc::new(react_core::interrupt::RejectPolicy)
        }
    } else {
        Arc::new(react_core::interrupt::WsInterruptPolicy)
    }
}

async fn emit_ws(
    state: &mut ConnState,
    write: &mut (impl SinkExt<Message> + Unpin),
    msg: api::ServerMessage,
) {
    if let Some(hub) = state.hub() {
        hub.emit(msg.clone());
    }
    if let Some(t) = state.term() {
        match &msg {
            api::ServerMessage::ThreadState(r) => {
                t.emit(TerminalEvent::ThreadState(r.state.clone()))
            }
            api::ServerMessage::Plans(r) => t.emit(TerminalEvent::Plans {
                thread_id: r.thread_id.clone(),
                plans: r.plans.clone(),
            }),
            api::ServerMessage::PlansChanged(r) => {
                t.emit(TerminalEvent::PlansChanged(r.clone()))
            }
            api::ServerMessage::Phase(r) => t.emit(TerminalEvent::Phase(r.clone())),
            api::ServerMessage::ToolStart(r) => t.emit(TerminalEvent::ToolStart(r.clone())),
            api::ServerMessage::ToolEnd(r) => t.emit(TerminalEvent::ToolEnd(r.clone())),
            api::ServerMessage::LlmStart(r) => t.emit(TerminalEvent::LlmStart(r.clone())),
            api::ServerMessage::LlmEnd(r) => t.emit(TerminalEvent::LlmEnd(r.clone())),
            _ => {}
        }
    }
    if let Ok(s) = serde_json::to_string(&msg) {
        state.buffer_last(&s);
        ws_log_out(&s);
        let _ = write.send(Message::Text(s)).await;
    }
}

async fn run_agent_with_processing_suite(
    thread_id: &str,
    question: &str,
    suite_id: &str,
    agent: &str,
    cid: &str,
    kind: SuiteRunKind,
    state: &mut ConnState,
    write: &mut (impl SinkExt<Message> + Unpin),
    interrupt_policy: &dyn InterruptPolicy,
) -> Result<(), String> {

    let sctx2 = state.suite_ctx.clone();

    let suite = state
        .reg
        .get(suite_id)
        .ok_or_else(|| format!("invalid suite_id '{}'", suite_id))?
        .clone();
    let spawn_task = |run_kind: SuiteRunKind, q: String| {
        let suite2 = suite.clone();
        let sctx3 = sctx2.clone();
        let thread_id_s = thread_id.to_string();
        let agent_s = agent.to_string();
        tokio::spawn(async move {
            let tid_scope = thread_id_s.clone();
            crate::llm::thread_ctx::scope_thread_id(&tid_scope, async move {
                match run_kind {
                    SuiteRunKind::New => {
                        suite2.handle_new(&thread_id_s, &q, &agent_s, &sctx3).await
                    }
                    SuiteRunKind::Open => {
                        suite2.handle_open(&thread_id_s, &q, &agent_s, &sctx3).await
                    }
                    SuiteRunKind::User => {
                        suite2.handle_user(&thread_id_s, &q, &agent_s, &sctx3).await
                    }
                }
            })
            .await
        })
    };

    let mut agent_task = spawn_task(kind, question.to_string());
    let mut _auto_turns: usize = 0;

    let mut plan_tick = tokio::time::interval(std::time::Duration::from_millis(800));
    plan_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_plan_fp_by_kind: HashMap<String, String> = HashMap::new();
    let mut last_plans: Vec<api::PlanSnapshot> = Vec::new();

    let mut last_emitted_step_idx: usize = match state.thread_store().get(thread_id).await {
        Ok(log) => log.steps.len(),
        Err(_) => 0,
    };
    let mut tool_tick = tokio::time::interval(std::time::Duration::from_millis(200));
    tool_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut last_state_sent: Option<api::ThreadStateSnapshot> = None;
    let mut last_sent_phase: Option<String> = None;
    let mut last_sent_step_count: Option<i32> = None;
    let mut state_tick = tokio::time::interval(std::time::Duration::from_millis(250));
    state_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = plan_tick.tick() => {
                let plans =
                    load_latest_plans(state.reg.as_ref(), suite_id, &state.suite_ctx, thread_id)
                        .await;
                {
                    fn semantic_plan_fp(p: &api::PlanSnapshot) -> String {
                        fn norm_checklist(items: &[api::PlanChecklistItem]) -> Vec<serde_json::Value> {
                            let mut out: Vec<serde_json::Value> = items
                                .iter()
                                .map(|it| {
                                    serde_json::json!({
                                        "checklist_item_id": it.checklist_item_id,
                                        "status": it.status
                                    })
                                })
                                .collect();
                            out.sort_by(|a, b| {
                                a.get("checklist_item_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .cmp(
                                        b.get("checklist_item_id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or(""),
                                    )
                            });
                            out
                        }

                        fn norm_task(t: &api::PlanTask) -> serde_json::Value {
                            serde_json::json!({
                                "task_kind": t.task_kind,
                                "task_id": t.task_id,
                                "label": t.label,
                                "status": t.status,
                                "checklist": norm_checklist(&t.checklist),
                                "details": t.details
                            })
                        }

                        let mut tasks = p.tasks.iter().map(norm_task).collect::<Vec<_>>();
                        tasks.sort_by(|a, b| {
                            a.get("task_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .cmp(b.get("task_id").and_then(|v| v.as_str()).unwrap_or(""))
                        });

                        let mut groups: Vec<serde_json::Value> = p
                            .work_groups
                            .iter()
                            .map(|g| {
                                let mut items = g
                                    .items
                                    .iter()
                                    .map(|it| serde_json::json!({
                                        "task_id": it.task_id,
                                        "checklist_item_id": it.checklist_item_id
                                    }))
                                    .collect::<Vec<_>>();
                                items.sort_by(|a, b| {
                                    let ak = format!(
                                        "{}:{}",
                                        a.get("task_id").and_then(|v| v.as_str()).unwrap_or(""),
                                        a.get("checklist_item_id").and_then(|v| v.as_str()).unwrap_or("")
                                    );
                                    let bk = format!(
                                        "{}:{}",
                                        b.get("task_id").and_then(|v| v.as_str()).unwrap_or(""),
                                        b.get("checklist_item_id").and_then(|v| v.as_str()).unwrap_or("")
                                    );
                                    ak.cmp(&bk)
                                });
                                let mut deps = g.depends_on_group_ids.clone().unwrap_or_default();
                                deps.sort();
                                deps.dedup();
                                serde_json::json!({
                                    "group_id": g.group_id,
                                    "kind": g.kind,
                                    "items": items,
                                    "depends_on_group_ids": deps
                                })
                            })
                            .collect();
                        groups.sort_by(|a, b| {
                            a.get("group_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .cmp(b.get("group_id").and_then(|v| v.as_str()).unwrap_or(""))
                        });

                        let norm = serde_json::json!({
                            "plan_kind": p.plan_kind,
                            "plan_key": p.plan_key,
                            "status": p.status,
                            "tasks": tasks,
                            "work_groups": groups
                        });
                        serde_json::to_string(&norm).unwrap_or_default()
                    }

                    let mut changed: Vec<String> = Vec::new();
                    let mut changed_plan_keys: Vec<String> = Vec::new();
                    for p in plans.iter() {
                        let kind = p.plan_kind.clone();
                        let new_fp = semantic_plan_fp(p);
                        let prev = last_plan_fp_by_kind.get(&kind);
                        if prev.map(|s| s.as_str()) != Some(new_fp.as_str()) {
                            changed.push(kind.clone());
                            changed_plan_keys.push(p.plan_key.clone());
                            last_plan_fp_by_kind.insert(kind, new_fp);
                        }
                    }
                    if !changed.is_empty() {
                        let mut ev = api::PlansChangedResponse::new(
                            1,
                            api::plans_changed_response::Type::PlansChanged,
                            now_iso(),
                            state.next_seq(),
                            thread_id.to_string(),
                            changed.clone(),
                        );
                        ev.for_cid = Some(cid.to_string());
                        ev.changed_plan_keys = Some(changed_plan_keys);
                        if let Some(t) = state.term() {
                            t.emit(TerminalEvent::PlansChanged(ev.clone()));
                        }
                        emit_ws(state, write, api::ServerMessage::PlansChanged(ev)).await;

                        let mut pr = api::PlansResponse::new(
                            1,
                            api::plans_response::Type::Plans,
                            now_iso(),
                            state.next_seq(),
                            thread_id.to_string(),
                            plans.clone(),
                        );
                        pr.for_cid = Some(cid.to_string());
                        emit_ws(state, write, api::ServerMessage::Plans(pr)).await;
                    }
                }
                last_plans = plans;
            }
            _ = tool_tick.tick() => {
                let store = state.thread_store();
                let log = match store.get(thread_id).await {
                    Ok(l) => l,
                    Err(_) => continue,
                };
                if last_emitted_step_idx >= log.steps.len() {
                    continue;
                }

                fn payload_map(v: &Option<serde_json::Value>) -> Option<std::collections::HashMap<String, serde_json::Value>> {
                    let obj = v.as_ref()?.as_object()?;
                    let mut out: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
                    for (k, vv) in obj.iter() {
                        out.insert(k.clone(), vv.clone());
                    }
                    Some(out)
                }

                for i in last_emitted_step_idx..log.steps.len() {
                    match &log.steps[i] {
                        ThreadStep::Phase { phase, from_phase, reason_code, reason_detail, ts, .. } => {
                            let runs_map = phase_runs_from_steps(&log.steps[..=i]);
                            let runs = runs_map.get(phase).cloned().unwrap_or_default();
                            let total_runtime_ms = total_completed_runtime_ms(&runs);
                            let (from_runs, from_total) =
                                match from_phase.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                                    Some(fp) => {
                                        let rr = runs_map.get(fp).cloned().unwrap_or_default();
                                        let tt = total_completed_runtime_ms(&rr);
                                        (Some(rr), Some(tt))
                                    }
                                    None => (None, None),
                                };

                            let mut ev = api::PhaseResponse::new(
                                1,
                                api::phase_response::Type::Phase,
                                now_iso(),
                                state.next_seq(),
                                thread_id.to_string(),
                                i as i32,
                                phase.clone(),
                                ts.clone(),
                                runs,
                                total_runtime_ms,
                            );
                            ev.for_cid = Some(cid.to_string());
                            ev.from_phase = from_phase.clone();
                            ev.reason_code = reason_code.clone();
                            ev.from_phase_runs = from_runs;
                            ev.from_phase_total_runtime_ms = from_total;
                            ev.reason_detail = reason_detail.as_ref().and_then(|v| {
                                if let Ok(rd) = serde_json::from_value::<api::PhaseReasonDetail>(v.clone()) {
                                    return Some(rd);
                                }
                                let mut rd = api::PhaseReasonDetail::new();
                                rd.data = v.as_object().map(|obj| {
                                    let mut hm: std::collections::HashMap<String, serde_json::Value> =
                                        std::collections::HashMap::new();
                                    for (k, vv) in obj.iter() {
                                        hm.insert(k.clone(), vv.clone());
                                    }
                                    hm
                                });
                                Some(rd)
                            });
                            if let Some(t) = state.term() {
                                t.emit(TerminalEvent::Phase(ev.clone()));
                            }
                            emit_ws(state, write, api::ServerMessage::Phase(ev)).await;
                            if let Ok(st) = store.get_thread_state(thread_id).await {
                                let timeline_events = load_timeline_events(&store, thread_id).await;
                                let snap = ws_thread_state_snapshot_from_core(&st, &timeline_events, state.reg.as_ref(), &last_plans);
                                if let Some(t) = state.term() {
                                    t.emit(TerminalEvent::ThreadState(snap.clone()));
                                }
                                let mut resp = api::ThreadStateResponse::new(
                                    1,
                                    m::thread_state_response::Type::ThreadState,
                                    now_iso(),
                                    state.next_seq(),
                                    thread_id.to_string(),
                                    snap,
                                );
                                resp.for_cid = Some(cid.to_string());
                                emit_ws(state, write, api::ServerMessage::ThreadState(resp)).await;
                            }
                        }
                        ThreadStep::ToolStart { tool_id, name, clean_name, status, payload, ctx, .. } => {
                            let st = match status {
                                ToolStepStatus::Running => api::ToolEventStatus::Running,
                                ToolStepStatus::Ok => api::ToolEventStatus::Ok,
                                ToolStepStatus::Failed => api::ToolEventStatus::Failed,
                            };
                            let mut ev = api::ToolStartResponse::new(
                                1,
                                api::tool_start_response::Type::ToolStart,
                                now_iso(),
                                state.next_seq(),
                                thread_id.to_string(),
                                tool_id.clone(),
                                name.clone(),
                                st,
                            );
                            ev.for_cid = Some(cid.to_string());
                            if !clean_name.trim().is_empty() {
                                ev.clean_name = Some(clean_name.clone());
                            }
                            ev.phase = phase_at_step_idx(&log.steps, i);
                            ev.payload = payload_map(payload);
                            ev.ctx = ctx.as_ref().map(map_exec_ctx);
                            if let Some(t) = state.term() {
                                t.emit(TerminalEvent::ToolStart(ev.clone()));
                            }
                            emit_ws(state, write, api::ServerMessage::ToolStart(ev)).await;
                        }
                        ThreadStep::ToolEnd { tool_id, name, clean_name, status, payload, ctx, observation, .. } => {
                            let st = match status {
                                ToolStepStatus::Running => api::ToolEventStatus::Running,
                                ToolStepStatus::Ok => api::ToolEventStatus::Ok,
                                ToolStepStatus::Failed => {
                                    if observation.ok {
                                        api::ToolEventStatus::Ok
                                    } else {
                                        api::ToolEventStatus::Failed
                                    }
                                }
                            };
                            let mut ev = api::ToolEndResponse::new(
                                1,
                                api::tool_end_response::Type::ToolEnd,
                                now_iso(),
                                state.next_seq(),
                                thread_id.to_string(),
                                tool_id.clone(),
                                name.clone(),
                                st,
                            );
                            ev.for_cid = Some(cid.to_string());
                            if !clean_name.trim().is_empty() {
                                ev.clean_name = Some(clean_name.clone());
                            }
                            ev.phase = phase_at_step_idx(&log.steps, i);
                            let mut pm: std::collections::HashMap<String, serde_json::Value> =
                                payload_map(payload).unwrap_or_default();
                            for (k, v) in &observation.extra {
                                if !pm.contains_key(k) {
                                    pm.insert(k.clone(), v.clone());
                                }
                            }
                            if !pm.is_empty() {
                                ev.payload = Some(pm);
                            } else {
                                ev.payload = None;
                            }
                            if !observation.ok {
                                ev.error = observation.errors.first().cloned();
                            }
                            ev.ctx = ctx.as_ref().map(map_exec_ctx);
                            if let Some(t) = state.term() {
                                t.emit(TerminalEvent::ToolEnd(ev.clone()));
                            }
                            emit_ws(state, write, api::ServerMessage::ToolEnd(ev)).await;
                        }
                        ThreadStep::LlmStart { call_id, phase, model, ctx, .. } => {
                            let mut ev = api::LlmStartResponse::new(
                                1,
                                api::llm_start_response::Type::LlmStart,
                                now_iso(),
                                state.next_seq(),
                                thread_id.to_string(),
                                *call_id as i32,
                                phase.clone(),
                            );
                            ev.for_cid = Some(cid.to_string());
                            ev.model = model.clone();
                            ev.ctx = ctx.as_ref().map(map_exec_ctx);
                            if let Some(t) = state.term() {
                                t.emit(TerminalEvent::LlmStart(ev.clone()));
                            }
                            emit_ws(state, write, api::ServerMessage::LlmStart(ev)).await;
                        }
                        ThreadStep::LlmEnd { call_id, phase, model, status, error, ctx, .. } => {
                            let mut ev = api::LlmEndResponse::new(
                                1,
                                api::llm_end_response::Type::LlmEnd,
                                now_iso(),
                                state.next_seq(),
                                thread_id.to_string(),
                                *call_id as i32,
                                phase.clone(),
                                if *status == react_core::session::LlmStepStatus::Failed {
                                    api::llm_end_response::Status::Failed
                                } else {
                                    api::llm_end_response::Status::Ok
                                },
                            );
                            ev.for_cid = Some(cid.to_string());
                            ev.model = model.clone();
                            ev.error = error.clone();
                            ev.ctx = ctx.as_ref().map(map_exec_ctx);
                            if let Some(t) = state.term() {
                                t.emit(TerminalEvent::LlmEnd(ev.clone()));
                            }
                            emit_ws(state, write, api::ServerMessage::LlmEnd(ev)).await;
                        }
                        _ => {}
                    }
                }
                last_emitted_step_idx = log.steps.len();
            }
            _ = state_tick.tick() => {
                let store = state.thread_store();
                let st = match store.get_thread_state(thread_id).await {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let timeline_events = load_timeline_events(&store, thread_id).await;
                let snap = ws_thread_state_snapshot_from_core(&st, &timeline_events, state.reg.as_ref(), &last_plans);
                let phase_changed = snap.current_phase.as_ref() != last_sent_phase.as_ref();
                let step_count_changed = Some(snap.last_materialized_step_count) != last_sent_step_count;
                let snapshot_changed = last_state_sent.as_ref() != Some(&snap);
                if !phase_changed && !step_count_changed && !snapshot_changed {
                    continue;
                }
                last_state_sent = Some(snap.clone());
                last_sent_phase = snap.current_phase.clone();
                last_sent_step_count = Some(snap.last_materialized_step_count);
                if let Some(t) = state.term() {
                    t.emit(TerminalEvent::ThreadState(snap.clone()));
                }
                let mut resp = api::ThreadStateResponse::new(
                    1,
                    m::thread_state_response::Type::ThreadState,
                    now_iso(),
                    state.next_seq(),
                    thread_id.to_string(),
                    snap,
                );
                resp.for_cid = Some(cid.to_string());
                emit_ws(state, write, api::ServerMessage::ThreadState(resp)).await;
            }
            res = &mut agent_task => {
                let frames = match res {
                    Ok(Ok(v)) => v,
                    Ok(Err(e)) => return Err(e),
                    Err(e) => return Err(format!("agent task failed: {}", e)),
                };
                let convert = |ff: react_core::suite::FlowFrame| -> AgentFrame {
                    match ff {
                        react_core::suite::FlowFrame::Complete { kind, payload, display } => AgentFrame::Final { kind: kind.0, payload, display },
                        react_core::suite::FlowFrame::Review { text, meta } => AgentFrame::Review { text, meta },
                        react_core::suite::FlowFrame::Checkpoint { kind, payload, display } => AgentFrame::Review { text: display.unwrap_or(kind.0), meta: Some(payload) },
                        react_core::suite::FlowFrame::Interrupt { kind, prompt } => {
                            if kind == "await_approval" {
                                AgentFrame::AwaitApproval { prompt }
                            } else {
                                AgentFrame::AwaitUser { prompt }
                            }
                        }
                    }
                };
                let frames = frames.into_iter().map(convert).collect::<Vec<_>>();
                let frames = frames;

                let mut rerun: Option<(SuiteRunKind, String)> = None;
                for f in frames {
                    match f {
                        AgentFrame::Review { text, meta } => {
                            let tseq = state.next_thread_seq(thread_id);
                            let mut resp = api::ReviewResponse::new(
                                1,
                                m::review_response::Type::Review,
                                now_iso(),
                                state.next_seq(),
                                thread_id.to_string(),
                                tseq,
                                text.clone(),
                            );
                            if let Some(v) = meta {
                                resp.meta = serde_json::from_value::<api::ReviewDecisionMeta>(v).ok();
                            }
                            emit_ws(state, write, api::ServerMessage::Review(resp)).await;

                            {
                                let store = state.thread_store();
                                if let Ok(st) = store.get_thread_state(thread_id).await {
                                    let timeline_events = load_timeline_events(&store, thread_id).await;
                                    let snap = ws_thread_state_snapshot_from_core(&st, &timeline_events, state.reg.as_ref(), &last_plans);
                                    if let Some(t) = state.term() {
                                        t.emit(TerminalEvent::ThreadState(snap.clone()));
                                    }
                                    let mut ts_resp = api::ThreadStateResponse::new(
                                        1,
                                        m::thread_state_response::Type::ThreadState,
                                        now_iso(),
                                        state.next_seq(),
                                        thread_id.to_string(),
                                        snap,
                                    );
                                    ts_resp.for_cid = Some(cid.to_string());
                                    emit_ws(state, write, api::ServerMessage::ThreadState(ts_resp)).await;
                                }
                            }

                            continue;
                        }
                        AgentFrame::Final { kind, payload, display } => {
                            {
                                let store = state.thread_store();
                                if let Ok(st) = store.get_thread_state(thread_id).await {
                                    let timeline_events = load_timeline_events(&store, thread_id).await;
                                    let snap = ws_thread_state_snapshot_from_core(&st, &timeline_events, state.reg.as_ref(), &last_plans);
                                    if let Some(t) = state.term() {
                                        t.emit(TerminalEvent::ThreadState(snap.clone()));
                                    }
                                    let mut resp = api::ThreadStateResponse::new(
                                        1,
                                        m::thread_state_response::Type::ThreadState,
                                        now_iso(),
                                        state.next_seq(),
                                        thread_id.to_string(),
                                        snap,
                                    );
                                    resp.for_cid = Some(cid.to_string());
                                    emit_ws(state, write, api::ServerMessage::ThreadState(resp)).await;
                                }
                            }

                            let tseq = state.next_thread_seq(thread_id);
                            let final_result = ws_final_result_from_typed_final(&kind, &payload, &display);
                            let resp = api::FinalResponse::new(
                                1,
                                m::final_response::Type::Final,
                                now_iso(),
                                state.next_seq(),
                                thread_id.to_string(),
                                tseq,
                                final_result,
                            );
                            emit_ws(state, write, api::ServerMessage::Final(resp)).await;

                            {
                                let store = state.thread_store();
                                log_thread_steps_if_enabled(&store, thread_id, "final").await;
                            }
                            return Ok(());
                        }
                        AgentFrame::AwaitUser { prompt } => {
                            match interrupt_policy.on_await_user(&prompt).await {
                                InterruptDecision::Reject { reason } => {
                                    if let Some(t) = state.term() {
                                        t.emit(TerminalEvent::Info(format!(
                                            "interrupt rejected: ask_user (prompt='{}')",
                                            truncate_str(&prompt, 220)
                                        )));
                                    }
                                    return Err(reason);
                                }
                                InterruptDecision::AutoApprove => {
                                    rerun = Some((SuiteRunKind::User, "Continue.".to_string()));
                                    break;
                                }
                                InterruptDecision::Forward => {}
                            }
                            let tseq = state.next_thread_seq(thread_id);
                            let resp = api::AwaitUserResponse::new(
                                1,
                                m::await_user_response::Type::AwaitUser,
                                now_iso(),
                                state.next_seq(),
                                thread_id.to_string(),
                                tseq,
                                prompt,
                            );
                            emit_ws(state, write, api::ServerMessage::AwaitUser(resp)).await;
                            {
                                let store = state.thread_store();
                                log_thread_steps_if_enabled(&store, thread_id, "await_user").await;
                            }
                            return Ok(());
                        }
                        AgentFrame::AwaitApproval { prompt } => {
                            match interrupt_policy.on_await_approval(&prompt).await {
                                InterruptDecision::AutoApprove => {
                                    _auto_turns += 1;
                                    {
                                        let store = state.thread_store();
                                        let _ = store
                                            .append_step(
                                                thread_id,
                                                ThreadStep::User {
                                                    text: "approve".to_string(),
                                                    observation: Observation::ok(),
                                                    ts: chrono::Utc::now().to_rfc3339(),
                                                    agent: agent.to_string(),
                                                },
                                            )
                                            .await;
                                    }
                                    if let Some(t) = state.term() {
                                        t.emit(TerminalEvent::Info(format!(
                                            "auto approve (prompt='{}')",
                                            prompt
                                        )));
                                    }
                                    rerun = Some((SuiteRunKind::User, "Continue.".to_string()));
                                    break;
                                }
                                InterruptDecision::Reject { reason } => {
                                    if let Some(t) = state.term() {
                                        t.emit(TerminalEvent::Info(format!(
                                            "interrupt rejected: await_approval (prompt='{}')",
                                            truncate_str(&prompt, 220)
                                        )));
                                    }
                                    return Err(reason);
                                }
                                InterruptDecision::Forward => {}
                            }
                            let tseq = state.next_thread_seq(thread_id);
                            let resp = api::AwaitApprovalResponse::new(
                                1,
                                m::await_approval_response::Type::AwaitApproval,
                                now_iso(),
                                state.next_seq(),
                                thread_id.to_string(),
                                tseq,
                                prompt,
                            );
                            emit_ws(state, write, api::ServerMessage::AwaitApproval(resp)).await;
                            {
                                let store = state.thread_store();
                                log_thread_steps_if_enabled(&store, thread_id, "await_approval").await;
                            }
                            return Ok(());
                        }
                    }
                }
                if let Some((k2, q2)) = rerun {
                    agent_task = spawn_task(k2, q2);
                    continue;
                }
                return Ok(());
            }
        }
    }
}

pub(super) async fn run_suite_and_frames(
    thread_id: &str,
    question: &str,
    suite_id: &str,
    agent: &str,
    reg: &react_core::suite::SuiteRegistry,
    sctx: &react_core::suite::SuiteCtx,
    kind: SuiteRunKind,
) -> Result<Vec<AgentFrame>, String> {
    let convert = |ff: react_core::suite::FlowFrame| -> AgentFrame {
        match ff {
            react_core::suite::FlowFrame::Complete {
                kind,
                payload,
                display,
            } => AgentFrame::Final {
                kind: kind.0,
                payload,
                display,
            },
            react_core::suite::FlowFrame::Review { text, meta } => AgentFrame::Review { text, meta },
            react_core::suite::FlowFrame::Checkpoint { kind, payload, display } => AgentFrame::Review { text: display.unwrap_or(kind.0), meta: Some(payload) },
            react_core::suite::FlowFrame::Interrupt { kind, prompt } => {
                if kind == "await_approval" {
                    AgentFrame::AwaitApproval { prompt }
                } else {
                    AgentFrame::AwaitUser { prompt }
                }
            }
        }
    };
    let suite = reg
        .get(suite_id)
        .ok_or_else(|| format!("invalid suite_id '{}'", suite_id))?;
    let frames = match kind {
        SuiteRunKind::New => suite.handle_new(thread_id, question, agent, sctx).await?,
        SuiteRunKind::Open => suite.handle_open(thread_id, question, agent, sctx).await?,
        SuiteRunKind::User => suite.handle_user(thread_id, question, agent, sctx).await?,
    }
        .into_iter()
        .map(convert)
        .collect();
    Ok(frames)
}
