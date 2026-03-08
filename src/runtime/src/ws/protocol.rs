use serde_json::Value;

use crate::models as m;
use crate::ws::api_gen::src::models as api;
use react_core::session::{Observation, ThreadStep};
use uuid::Uuid;

use super::conn_state::{
    build_suites_catalog, default_suite_id, load_latest_plans,
    normalize_agent_new, normalize_agent_open, resolve_suite_id_for_thread,
    resolve_thread_context, synthesize_title, ConnState,
};
use super::history::{build_history, compute_unread_for_log};
use super::mapping::{final_display_text_from_payload, ws_final_result_from_typed_final};
use super::suite_runner::{run_suite_and_frames, AgentFrame, SuiteRunKind};
use super::terminal::TerminalEvent;
use super::thread_state::{
    load_timeline_events, ws_thread_state_snapshot_from_core,
};
use super::util::{now_iso, truncate_title, DEFAULT_AGENT_TYPE, DEFAULT_INITIAL_PHASE};

pub(super) async fn handle_message(text: &str, state: &mut ConnState) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let typ = v
        .get("type")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "missing field `type`".to_string())?;
    match typ {
        "list" => handle_list_message(&v, state).await,
        "suites" => handle_suites_message(&v, state).await,
        "new" => handle_new_message(&v, state).await,
        "open" => handle_open_message(&v, state).await,
        "user" => handle_user_message(&v, state).await,
        "history" => handle_history_message(&v, state).await,
        "seen" => handle_seen_message(&v, state).await,
        "plans" => handle_plans_message(&v, state).await,
        "thread_state" => handle_thread_state_message(&v, state).await,
        "delete" => handle_delete_message(&v, state).await,
        _ => Err("unknown type".to_string()),
    }
}

async fn handle_list_message(v: &Value, state: &mut ConnState) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let _req: api::ListRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let ids = {
        let store = state.thread_store();
        store.list().await
    };
    let mut threads: Vec<api::ListResponseThreadsInner> = Vec::new();
    for tid in ids {
        let mut item = api::ListResponseThreadsInner::new(tid.clone());
        let (suite_id, agent_type) = resolve_thread_context(state, &tid).await;
        let store = state.thread_store();
        if let Ok(log) = store.get(&tid).await {
            item.last_activity = log.steps.last().map(|s| s.ts().to_string());
            item.title = log.title.clone();
            item.suite_id = Some(suite_id);
            item.agent_type = Some(agent_type);
            let mut preview: Option<String> = None;
            for step in log.steps.iter().rev() {
                match step {
                    ThreadStep::User { text, .. } => {
                        preview = Some(text.to_string());
                        break;
                    }
                    ThreadStep::Complete {
                        payload, display, ..
                    } => {
                        preview =
                            Some(display.clone().unwrap_or_else(|| {
                                final_display_text_from_payload(payload)
                            }));
                        break;
                    }
                    _ => {}
                }
            }
            item.last_message_preview = preview;
            let seen = state.seen.get(&tid).copied().unwrap_or(0);
            let (assistant_seq_max, assistant_after_seen) =
                compute_unread_for_log(&log, seen);
            let unread = assistant_after_seen;
            item.unread_count = Some(unread);
            let entry = state.thread_seq.entry(tid.clone()).or_insert(0);
            if *entry < assistant_seq_max {
                *entry = assistant_seq_max;
            }
        }
        threads.push(item);
    }
    let resp = api::ListResponse::new(
        1,
        m::list_response::Type::List,
        now_iso(),
        state.next_seq(),
        threads,
    );
    out.push(serde_json::to_string(&resp).unwrap_or_else(|_| {
        "{\"type\":\"error\",\"error\":\"serialization error\"}".to_string()
    }));
    state.buffer_last(&out[out.len() - 1]);
    Ok(out)
}

async fn handle_suites_message(v: &Value, state: &mut ConnState) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let _req: api::SuitesRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let suites = build_suites_catalog(&state.reg);
    let mut resp = api::SuitesResponse::new(
        1,
        m::suites_response::Type::Suites,
        now_iso(),
        state.next_seq(),
        suites,
    );
    resp.default_suite_id = default_suite_id(&state.reg);
    let s = serde_json::to_string(&resp).unwrap();
    state.buffer_last(&s);
    out.push(s);
    Ok(out)
}

async fn handle_new_message(v: &Value, state: &mut ConnState) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let req: api::NewRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let cid = req.cid.clone();
    let question = req.question.clone().unwrap_or_default();
    let thread_id = Uuid::new_v4().to_string();
    let suite_id = req.suite_id.clone();
    let agent = normalize_agent_new(req.agent_type);
    state
        .current_suite
        .insert(thread_id.clone(), suite_id.clone());
    state.current_agent.insert(thread_id.clone(), agent.clone());
    {
        let store = state.thread_store();
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::SwitchSuite {
                    from: None,
                    to: suite_id.clone(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: agent.clone(),
                },
            )
            .await;
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::SwitchAgent {
                    from: None,
                    to: agent.clone(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: agent.clone(),
                },
            )
            .await;
    }
    {
        let store = state.thread_store();
        if let Ok(Some((step_idx, ts))) =
            store.ensure_preflight_phase_step(&thread_id, &agent, Some(&suite_id), DEFAULT_INITIAL_PHASE).await
        {
            let runs = vec![api::PhaseRun::new(ts.clone())];
            let mut ev = api::PhaseResponse::new(
                1,
                api::phase_response::Type::Phase,
                now_iso(),
                state.next_seq(),
                thread_id.clone(),
                step_idx as i32,
                DEFAULT_INITIAL_PHASE.to_string(),
                ts,
                runs,
                0,
            );
            ev.for_cid = Some(cid.clone());
            ev.reason_code = Some(
                "preflight_start".to_string(),
            );
            let s = serde_json::to_string(&api::ServerMessage::Phase(ev)).unwrap();
            state.buffer_last(&s);
            out.push(s);
        }
    }
    let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
    ok.cid = Some(cid.clone());
    out.push(serde_json::to_string(&api::ServerMessage::Ok(ok)).unwrap());
    let ta = api::ServerMessage::ThreadAssigned(api::ThreadAssignedResponse::new(
        1,
        m::thread_assigned_response::Type::ThreadAssigned,
        now_iso(),
        state.next_seq(),
        cid.clone(),
        thread_id.clone(),
    ));
    let ta_s = serde_json::to_string(&ta).unwrap();
    state.buffer_last(&ta_s);
    out.push(ta_s);
    if !question.trim().is_empty() {
        {
            let store = state.thread_store();
            let _ = store
                .append_step(
                    &thread_id,
                    ThreadStep::User {
                        text: question.clone(),
                        observation: Observation::ok(),
                        ts: chrono::Utc::now().to_rfc3339(),
                        agent: agent.clone(),
                    },
                )
                .await;
            let _ = store
                .set_title_if_absent(&thread_id, &truncate_title(&question, 64))
                .await;
        }
        let frames = run_suite_and_frames(
            &thread_id,
            &question,
            &suite_id,
            &agent,
            &state.reg,
            &state.suite_ctx,
            SuiteRunKind::New,
        )
        .await?;
        emit_agent_frames(&mut out, state, &thread_id, &agent, &question, frames).await;
    }
    Ok(out)
}

async fn handle_open_message(v: &Value, state: &mut ConnState) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let req: api::OpenRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let cid = req.cid.clone();
    let thread_id = req.thread_id.clone();
    if thread_id.is_empty() {
        return Err("thread_id required".into());
    }
    if uuid::Uuid::parse_str(&thread_id).is_err() {
        return Err("invalid thread_id".into());
    }
    let question = req
        .question
        .clone()
        .unwrap_or_else(|| "Continue.".to_string());
    let requested_suite = req.suite_id.clone();
    let requested_agent = normalize_agent_open(req.agent_type);

    let (current_suite, current_agent) = resolve_thread_context(state, &thread_id).await;
    let store = state.thread_store();
    if current_suite != requested_suite {
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::SwitchSuite {
                    from: Some(current_suite.clone()),
                    to: requested_suite.clone(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: requested_agent.clone(),
                },
            )
            .await;
    }
    state
        .current_suite
        .insert(thread_id.clone(), requested_suite.clone());

    if current_agent != requested_agent {
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::SwitchAgent {
                    from: Some(current_agent.clone()),
                    to: requested_agent.clone(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: requested_agent.clone(),
                },
            )
            .await;
        state
            .current_agent
            .insert(thread_id.clone(), requested_agent.clone());
    }
    state
        .current_agent
        .insert(thread_id.clone(), requested_agent.clone());
    let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
    ok.cid = Some(cid.clone());
    out.push(serde_json::to_string(&api::ServerMessage::Ok(ok)).unwrap());
    let suite_id = state
        .current_suite
        .get(&thread_id)
        .cloned()
        .unwrap_or_else(|| requested_suite.clone());
    let agent = state
        .current_agent
        .get(&thread_id)
        .cloned()
        .unwrap_or_else(|| requested_agent.clone());
    {
        let store = state.thread_store();
        if let Ok(Some((step_idx, ts))) =
            store.ensure_preflight_phase_step(&thread_id, &agent, Some(&suite_id), DEFAULT_INITIAL_PHASE).await
        {
            let runs = vec![api::PhaseRun::new(ts.clone())];
            let mut ev = api::PhaseResponse::new(
                1,
                api::phase_response::Type::Phase,
                now_iso(),
                state.next_seq(),
                thread_id.clone(),
                step_idx as i32,
                DEFAULT_INITIAL_PHASE.to_string(),
                ts,
                runs,
                0,
            );
            ev.for_cid = Some(cid.clone());
            ev.reason_code = Some(
                "preflight_start".to_string(),
            );
            let s = serde_json::to_string(&api::ServerMessage::Phase(ev)).unwrap();
            state.buffer_last(&s);
            out.push(s);
        }
    }
    let frames = run_suite_and_frames(
        &thread_id,
        &question,
        &suite_id,
        &agent,
        &state.reg,
        &state.suite_ctx,
        SuiteRunKind::Open,
    )
    .await?;
    for f in frames {
        match f {
            AgentFrame::Review { text, meta } => {
                let tseq = state.next_thread_seq(&thread_id);
                let mut rr = api::ReviewResponse::new(
                    1,
                    m::review_response::Type::Review,
                    now_iso(),
                    state.next_seq(),
                    thread_id.clone(),
                    tseq,
                    text.clone(),
                );
                if let Some(v) = meta {
                    rr.meta = serde_json::from_value::<api::ReviewDecisionMeta>(v).ok();
                }
                let resp = api::ServerMessage::Review(rr.clone());
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
                {
                    let store = state.thread_store();
                    let meta_val: Option<Value> =
                        rr.meta.as_ref().and_then(|m| serde_json::to_value(m).ok());
                    let _ = store
                        .append_step(
                            &thread_id,
                            ThreadStep::ReviewResponse {
                                text: text.clone(),
                                meta: meta_val,
                                observation: Observation::ok(),
                                ts: chrono::Utc::now().to_rfc3339(),
                                agent: agent.clone(),
                            },
                        )
                        .await;
                }
            }
            AgentFrame::Final {
                kind,
                payload,
                display,
            } => {
                let tseq = state.next_thread_seq(&thread_id);
                let final_result =
                    ws_final_result_from_typed_final(&kind, &payload, &display);
                let resp = api::ServerMessage::Final(api::FinalResponse::new(
                    1,
                    m::final_response::Type::Final,
                    now_iso(),
                    state.next_seq(),
                    thread_id.clone(),
                    tseq,
                    final_result,
                ));
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
                {
                    let store = state.thread_store();
                    if let Ok(log) = store.get(&thread_id).await {
                        if let Ok(pretty) = serde_json::to_string_pretty(&log) {
                            tracing::info!("{}", pretty);
                        }
                    }
                }
            }
            AgentFrame::AwaitUser { prompt } => {
                let tseq = state.next_thread_seq(&thread_id);
                let resp = api::ServerMessage::AwaitUser(api::AwaitUserResponse::new(
                    1,
                    m::await_user_response::Type::AwaitUser,
                    now_iso(),
                    state.next_seq(),
                    thread_id.clone(),
                    tseq,
                    prompt.clone(),
                ));
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
                {
                    let store = state.thread_store();
                    let _ = store
                        .append_step(
                            &thread_id,
                            ThreadStep::Interrupt {
                                kind: "await_user".to_string(),
                                prompt,
                                observation: Observation::ok(),
                                ts: chrono::Utc::now().to_rfc3339(),
                                agent: agent.clone(),
                            },
                        )
                        .await;
                }
            }
            AgentFrame::AwaitApproval { prompt } => {
                let tseq = state.next_thread_seq(&thread_id);
                let resp =
                    api::ServerMessage::AwaitApproval(api::AwaitApprovalResponse::new(
                        1,
                        m::await_approval_response::Type::AwaitApproval,
                        now_iso(),
                        state.next_seq(),
                        thread_id.clone(),
                        tseq,
                        prompt,
                    ));
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
            }
        }
    }
    Ok(out)
}

async fn handle_user_message(v: &Value, state: &mut ConnState) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let req: api::UserRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let thread_id = req.thread_id.clone();
    let text = req.text.clone();
    if thread_id.is_empty() || text.trim().is_empty() {
        return Err("thread_id and text required".into());
    }
    if uuid::Uuid::parse_str(&thread_id).is_err() {
        return Err("invalid thread_id".into());
    }
    let (_suite_id, agent_label) = resolve_thread_context(state, &thread_id).await;
    let store = state.thread_store();
    let _ = store
        .append_step(
            &thread_id,
            ThreadStep::User {
                text: text.clone(),
                observation: Observation::ok(),
                ts: chrono::Utc::now().to_rfc3339(),
                agent: agent_label.clone(),
            },
        )
        .await;
    let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
    ok.cid = Some(req.cid.clone());
    out.push(serde_json::to_string(&api::ServerMessage::Ok(ok)).unwrap());
    let suite_id = state
        .current_suite
        .get(&thread_id)
        .cloned()
        .ok_or_else(|| "suite_id missing for thread".to_string())?;
    let agent = state
        .current_agent
        .get(&thread_id)
        .cloned()
        .ok_or_else(|| "agent_type missing for thread".to_string())?;
    tracing::info!(
        "user auto-resume (non-streaming fallback): thread_id={} agent={}",
        thread_id,
        agent
    );
    let frames = run_suite_and_frames(
        &thread_id,
        &text,
        &suite_id,
        &agent,
        &state.reg,
        &state.suite_ctx,
        SuiteRunKind::User,
    )
    .await?;
    for f in frames {
        match f {
            AgentFrame::Review { text, meta } => {
                let tseq = state.next_thread_seq(&thread_id);
                let mut rr = api::ReviewResponse::new(
                    1,
                    m::review_response::Type::Review,
                    now_iso(),
                    state.next_seq(),
                    thread_id.clone(),
                    tseq,
                    text.clone(),
                );
                if let Some(v) = meta {
                    rr.meta = serde_json::from_value::<api::ReviewDecisionMeta>(v).ok();
                }
                let resp = api::ServerMessage::Review(rr.clone());
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
                {
                    let store = state.thread_store();
                    let agent_label = state
                        .current_agent
                        .get(&thread_id)
                        .cloned()
                        .unwrap_or_else(|| DEFAULT_AGENT_TYPE.to_string());
                    let meta_val: Option<Value> =
                        rr.meta.as_ref().and_then(|m| serde_json::to_value(m).ok());
                    let _ = store
                        .append_step(
                            &thread_id,
                            ThreadStep::ReviewResponse {
                                text: text.clone(),
                                meta: meta_val,
                                observation: Observation::ok(),
                                ts: chrono::Utc::now().to_rfc3339(),
                                agent: agent_label,
                            },
                        )
                        .await;
                }
            }
            AgentFrame::Final {
                kind,
                payload,
                display,
            } => {
                let tseq = state.next_thread_seq(&thread_id);
                let final_result =
                    ws_final_result_from_typed_final(&kind, &payload, &display);
                let resp = api::ServerMessage::Final(api::FinalResponse::new(
                    1,
                    m::final_response::Type::Final,
                    now_iso(),
                    state.next_seq(),
                    thread_id.clone(),
                    tseq,
                    final_result,
                ));
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
                if let Ok(log) = store.get(&thread_id).await {
                    if let Ok(pretty) = serde_json::to_string_pretty(&log) {
                        tracing::info!("{}", pretty);
                    }
                }
            }
            AgentFrame::AwaitUser { prompt } => {
                let tseq = state.next_thread_seq(&thread_id);
                let resp = api::ServerMessage::AwaitUser(api::AwaitUserResponse::new(
                    1,
                    m::await_user_response::Type::AwaitUser,
                    now_iso(),
                    state.next_seq(),
                    thread_id.clone(),
                    tseq,
                    prompt.clone(),
                ));
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
            }
            AgentFrame::AwaitApproval { prompt } => {
                let tseq = state.next_thread_seq(&thread_id);
                let resp =
                    api::ServerMessage::AwaitApproval(api::AwaitApprovalResponse::new(
                        1,
                        m::await_approval_response::Type::AwaitApproval,
                        now_iso(),
                        state.next_seq(),
                        thread_id.clone(),
                        tseq,
                        prompt,
                    ));
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
            }
        }
    }
    Ok(out)
}

async fn handle_history_message(v: &Value, state: &mut ConnState) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let req: api::HistoryRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let thread_id = req.thread_id.clone();
    if thread_id.is_empty() {
        return Err("thread_id required".into());
    }
    if uuid::Uuid::parse_str(&thread_id).is_err() {
        return Err("invalid thread_id".into());
    }
    let (suite_id, agent_type) = resolve_thread_context(state, &thread_id).await;
    let store = state.thread_store();
    let (messages, next_before) =
        build_history(&store, &thread_id, req.before_thread_seq, req.limit).await?;
    let mut resp = api::HistoryResponse::new(
        1,
        m::history_response::Type::History,
        now_iso(),
        state.next_seq(),
        thread_id.clone(),
        messages,
    );
    let log = store.get(&thread_id).await.map_err(|e| e.to_string())?;
    resp.title = log.title;
    resp.suite_id = Some(suite_id);
    resp.agent_type = Some(agent_type);
    resp.next_before_thread_seq = next_before;
    let outm = api::ServerMessage::History(resp);
    let s = serde_json::to_string(&outm).unwrap();
    state.buffer_last(&s);
    out.push(s);
    Ok(out)
}

async fn handle_seen_message(v: &Value, state: &mut ConnState) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let req: api::SeenRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let thread_id = req.thread_id.clone();
    state.seen.insert(thread_id.clone(), req.up_to_thread_seq);
    let store = state.thread_store();
    let log = store.get(&thread_id).await.map_err(|e| e.to_string())?;
    let (_max_assistant, unread) = compute_unread_for_log(&log, req.up_to_thread_seq);
    let resp = api::ServerMessage::Unread(api::UnreadResponse::new(
        1,
        m::unread_response::Type::Unread,
        now_iso(),
        state.next_seq(),
        thread_id.clone(),
        unread,
    ));
    let s = serde_json::to_string(&resp).unwrap();
    state.buffer_last(&s);
    out.push(s);
    Ok(out)
}

async fn handle_plans_message(v: &Value, state: &mut ConnState) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let req: api::PlansRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let thread_id = req.thread_id.clone();
    if thread_id.is_empty() {
        return Err("thread_id required".into());
    }
    if uuid::Uuid::parse_str(&thread_id).is_err() {
        return Err("invalid thread_id".into());
    }

    let suite_id = resolve_suite_id_for_thread(state, &thread_id).await;
    let plans =
        load_latest_plans(state.reg.as_ref(), &suite_id, &state.suite_ctx, &thread_id)
            .await;
    let mut resp = api::PlansResponse::new(
        1,
        m::plans_response::Type::Plans,
        now_iso(),
        state.next_seq(),
        thread_id.clone(),
        plans,
    );
    resp.for_cid = Some(req.cid.clone());
    if let Some(t) = state.term() {
        t.emit(TerminalEvent::Plans {
            thread_id: thread_id.clone(),
            plans: resp.plans.clone(),
        });
    }
    let s = serde_json::to_string(&resp).unwrap();
    state.buffer_last(&s);
    out.push(s);
    Ok(out)
}

async fn handle_thread_state_message(v: &Value, state: &mut ConnState) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let req: api::ThreadStateRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let thread_id = req.thread_id.clone();
    if thread_id.is_empty() {
        return Err("thread_id required".into());
    }
    if uuid::Uuid::parse_str(&thread_id).is_err() {
        return Err("invalid thread_id".into());
    }

    let suite_id = resolve_suite_id_for_thread(state, &thread_id).await;
    let plans = load_latest_plans(state.reg.as_ref(), &suite_id, &state.suite_ctx, &thread_id).await;
    let store = state.thread_store();
    let st = store.get_thread_state(&thread_id).await.map_err(|e| e.to_string())?;
    let timeline_events = load_timeline_events(&store, &thread_id).await;
    let snap = ws_thread_state_snapshot_from_core(&st, &timeline_events, state.reg.as_ref(), &plans);
    if let Some(t) = state.term() {
        t.emit(TerminalEvent::ThreadState(snap.clone()));
    }
    let mut resp = api::ThreadStateResponse::new(
        1,
        m::thread_state_response::Type::ThreadState,
        now_iso(),
        state.next_seq(),
        thread_id.clone(),
        snap,
    );
    resp.for_cid = Some(req.cid.clone());
    let s = serde_json::to_string(&resp).unwrap();
    state.buffer_last(&s);
    out.push(s);
    Ok(out)
}

async fn handle_delete_message(v: &Value, state: &mut ConnState) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let req: api::DeleteRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let cid = req.cid.clone();
    let thread_id = req.thread_id.clone();
    if thread_id.is_empty() {
        return Err("thread_id required".into());
    }
    if uuid::Uuid::parse_str(&thread_id).is_err() {
        return Err("invalid thread_id".into());
    }
    {
        let store = state.thread_store();
        let _ = store.delete(&thread_id).await;
    }
    if let Some(vs) = state.suite_ctx.vector().as_ref() {
        let _ = vs
            .delete_thread_embeddings(&state.suite_ctx.scope(), &thread_id)
            .await;
    }
    let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
    ok.cid = Some(cid.clone());
    out.push(serde_json::to_string(&api::ServerMessage::Ok(ok)).unwrap());
    Ok(out)
}

/// Shared helper for emitting agent frames with title synthesis (used by `new` handler).
async fn emit_agent_frames(
    out: &mut Vec<String>,
    state: &mut ConnState,
    thread_id: &str,
    agent: &str,
    question: &str,
    frames: Vec<AgentFrame>,
) {
    for f in frames {
        match f {
            AgentFrame::Review { text, meta } => {
                let tseq = state.next_thread_seq(thread_id);
                let mut rr = api::ReviewResponse::new(
                    1,
                    m::review_response::Type::Review,
                    now_iso(),
                    state.next_seq(),
                    thread_id.to_string(),
                    tseq,
                    text.clone(),
                );
                if let Some(v) = meta {
                    rr.meta = serde_json::from_value::<api::ReviewDecisionMeta>(v).ok();
                }
                let resp = api::ServerMessage::Review(rr.clone());
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
                {
                    let store = state.thread_store();
                    let meta_val: Option<Value> =
                        rr.meta.as_ref().and_then(|m| serde_json::to_value(m).ok());
                    let _ = store
                        .append_step(
                            thread_id,
                            ThreadStep::ReviewResponse {
                                text: text.clone(),
                                meta: meta_val,
                                observation: Observation::ok(),
                                ts: chrono::Utc::now().to_rfc3339(),
                                agent: agent.to_string(),
                            },
                        )
                        .await;
                }
            }
            AgentFrame::Final {
                kind,
                payload,
                display,
            } => {
                let display_text = display
                    .clone()
                    .unwrap_or_else(|| final_display_text_from_payload(&payload));
                {
                    let store = state.thread_store();
                    let title = synthesize_title(
                        &state.suite_ctx.llm(),
                        question,
                        &display_text,
                    )
                    .await;
                    let _ = store.lock_title(thread_id, &title).await;
                }
                {
                    let store = state.thread_store();
                    store.append_step_if_new(
                        thread_id,
                        ThreadStep::Complete {
                            kind: kind.clone(),
                            payload: payload.clone(),
                            display: display.clone(),
                            observation: Observation::ok(),
                            ts: chrono::Utc::now().to_rfc3339(),
                            agent: agent.to_string(),
                        },
                    )
                    .await;
                }
                let tseq = state.next_thread_seq(thread_id);
                let final_result =
                    ws_final_result_from_typed_final(&kind, &payload, &display);
                let resp = api::ServerMessage::Final(api::FinalResponse::new(
                    1,
                    m::final_response::Type::Final,
                    now_iso(),
                    state.next_seq(),
                    thread_id.to_string(),
                    tseq,
                    final_result,
                ));
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
                {
                    let store = state.thread_store();
                    if let Ok(log) = store.get(thread_id).await {
                        if let Ok(pretty) = serde_json::to_string_pretty(&log) {
                            tracing::info!("{}", pretty);
                        }
                    }
                }
            }
            AgentFrame::AwaitUser { prompt } => {
                let tseq = state.next_thread_seq(thread_id);
                let resp = api::ServerMessage::AwaitUser(api::AwaitUserResponse::new(
                    1,
                    m::await_user_response::Type::AwaitUser,
                    now_iso(),
                    state.next_seq(),
                    thread_id.to_string(),
                    tseq,
                    prompt.clone(),
                ));
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
                {
                    let store = state.thread_store();
                    store.append_step_if_new(
                        thread_id,
                        ThreadStep::Interrupt {
                            kind: "await_user".to_string(),
                            prompt,
                            observation: Observation::ok(),
                            ts: chrono::Utc::now().to_rfc3339(),
                            agent: agent.to_string(),
                        },
                    )
                    .await;
                }
            }
            AgentFrame::AwaitApproval { prompt } => {
                let tseq = state.next_thread_seq(thread_id);
                let resp =
                    api::ServerMessage::AwaitApproval(api::AwaitApprovalResponse::new(
                        1,
                        m::await_approval_response::Type::AwaitApproval,
                        now_iso(),
                        state.next_seq(),
                        thread_id.to_string(),
                        tseq,
                        prompt.clone(),
                    ));
                let s = serde_json::to_string(&resp).unwrap();
                state.buffer_last(&s);
                out.push(s);
                {
                    let store = state.thread_store();
                    store.append_step_if_new(
                        thread_id,
                        ThreadStep::Interrupt {
                            kind: "await_approval".to_string(),
                            prompt,
                            observation: Observation::ok(),
                            ts: chrono::Utc::now().to_rfc3339(),
                            agent: agent.to_string(),
                        },
                    )
                    .await;
                }
            }
        }
    }
}
