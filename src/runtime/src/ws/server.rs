use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use crate::models as m;
use crate::run::event_hub::EventHub;
use crate::ws::api_gen::src::models as api;
use crate::ws::terminal::{self, TerminalEvent};
use react_core::session::{Observation, ThreadStep};
use react_core::suite::{SuiteCtx, SuiteRegistry};
use std::sync::Arc;
use uuid::Uuid;

use super::conn_state::{
    build_suites_catalog, default_suite_id, derive_thread_context, load_latest_plans,
    normalize_agent_new, normalize_agent_open, resolve_suite_id_for_thread, synthesize_title,
    ConnState,
};
use super::history::{build_history, compute_unread_for_log};
use super::mapping::{final_display_text_from_payload, ws_final_result_from_typed_final};
use super::suite_runner::{run_suite_and_frames, run_suite_and_stream, AgentFrame, SuiteRunKind};
use super::thread_state::{
    append_step_if_new, ensure_preflight_phase_step, load_timeline_events,
    upsert_thread_state_from_plans, ws_thread_state_snapshot_from_core,
};
use super::util::{now_iso, truncate_title, ws_log_in, ws_log_out};

pub async fn start_with_ctx(port: u16, suite_ctx: SuiteCtx, registry: SuiteRegistry) -> Result<(), String> {
    let reg = Arc::new(registry);
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await.map_err(|e| e.to_string())?;
    tracing::info!("WebSocket server listening on ws://{}", addr);
    if let Some(t) = terminal::sink() {
        t.emit(TerminalEvent::Info(format!(
            "WS server listening on ws://{}",
            addr
        )));
    }
    loop {
        let (stream, _sockaddr) = listener.accept().await.map_err(|e| e.to_string())?;
        let reg = reg.clone();
        let suite_ctx = suite_ctx.clone();
        tokio::spawn(async move {
            if let Ok(ws_stream) = tokio_tungstenite::accept_async(stream).await {
                let (mut write, mut read) = ws_stream.split();
                let mut state = ConnState::new(reg, suite_ctx, None);
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(txt)) => {
                            ws_log_in(&txt);
                            // Fast-path new/open to stream initial frames immediately
                            if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                                if let Some(t) = v.get("type").and_then(|x| x.as_str()) {
                                    if t == "new" {
                                        if let Err(e) =
                                            process_new(&v, &mut state, &mut write).await
                                        {
                                            let cid_guess = v
                                                .get("cid")
                                                .and_then(|x| x.as_str())
                                                .map(|s| s.to_string());
                                            let mut err = api::ErrorResponse::new(
                                                1,
                                                m::error_response::Type::Error,
                                                now_iso(),
                                                e.clone(),
                                            );
                                            err.code = Some("invalid_request".to_string());
                                            err.cid = cid_guess;
                                            let s = serde_json::to_string(&err).unwrap_or_else(|_| "{\"v\":1,\"type\":\"error\",\"server_time\":\"\",\"error\":\"internal\"}".to_string());
                                            ws_log_out(&s);
                                            let _ = write.send(Message::Text(s)).await;
                                        }
                                        continue;
                                    } else if t == "open" {
                                        if let Err(e) =
                                            process_open(&v, &mut state, &mut write).await
                                        {
                                            let cid_guess = v
                                                .get("cid")
                                                .and_then(|x| x.as_str())
                                                .map(|s| s.to_string());
                                            let mut err = api::ErrorResponse::new(
                                                1,
                                                m::error_response::Type::Error,
                                                now_iso(),
                                                e.clone(),
                                            );
                                            err.code = Some("invalid_request".to_string());
                                            err.cid = cid_guess;
                                            let s = serde_json::to_string(&err).unwrap_or_else(|_| "{\"v\":1,\"type\":\"error\",\"server_time\":\"\",\"error\":\"internal\"}".to_string());
                                            ws_log_out(&s);
                                            let _ = write.send(Message::Text(s)).await;
                                        }
                                        continue;
                                    } else if t == "user" {
                                        if let Err(e) =
                                            process_user(&v, &mut state, &mut write).await
                                        {
                                            let cid_guess = v
                                                .get("cid")
                                                .and_then(|x| x.as_str())
                                                .map(|s| s.to_string());
                                            let mut err = api::ErrorResponse::new(
                                                1,
                                                m::error_response::Type::Error,
                                                now_iso(),
                                                e.clone(),
                                            );
                                            err.code = Some("invalid_request".to_string());
                                            err.cid = cid_guess;
                                            let s = serde_json::to_string(&err).unwrap_or_else(|_| "{\"v\":1,\"type\":\"error\",\"server_time\":\"\",\"error\":\"internal\"}".to_string());
                                            ws_log_out(&s);
                                            let _ = write.send(Message::Text(s)).await;
                                        }
                                        continue;
                                    } else if t == "approve" {
                                        if let Err(e) =
                                            process_approve(&v, &mut state, &mut write).await
                                        {
                                            let cid_guess = v
                                                .get("cid")
                                                .and_then(|x| x.as_str())
                                                .map(|s| s.to_string());
                                            let mut err = api::ErrorResponse::new(
                                                1,
                                                m::error_response::Type::Error,
                                                now_iso(),
                                                e.clone(),
                                            );
                                            err.code = Some("invalid_request".to_string());
                                            err.cid = cid_guess;
                                            let s = serde_json::to_string(&err).unwrap_or_else(|_| "{\"v\":1,\"type\":\"error\",\"server_time\":\"\",\"error\":\"internal\"}".to_string());
                                            ws_log_out(&s);
                                            let _ = write.send(Message::Text(s)).await;
                                        }
                                        continue;
                                    } else if t == "reject" {
                                        if let Err(e) =
                                            process_reject(&v, &mut state, &mut write).await
                                        {
                                            let cid_guess = v
                                                .get("cid")
                                                .and_then(|x| x.as_str())
                                                .map(|s| s.to_string());
                                            let mut err = api::ErrorResponse::new(
                                                1,
                                                m::error_response::Type::Error,
                                                now_iso(),
                                                e.clone(),
                                            );
                                            err.code = Some("invalid_request".to_string());
                                            err.cid = cid_guess;
                                            let s = serde_json::to_string(&err).unwrap_or_else(|_| "{\"v\":1,\"type\":\"error\",\"server_time\":\"\",\"error\":\"internal\"}".to_string());
                                            ws_log_out(&s);
                                            let _ = write.send(Message::Text(s)).await;
                                        }
                                        continue;
                                    }
                                }
                            }
                            // Other types: handle and send after processing
                            match handle_message(&txt, &mut state).await {
                                Ok(frames) => {
                                    for f in frames {
                                        if let Some(t) = state.term() {
                                            t.emit(TerminalEvent::RawJson(f.clone()));
                                        }
                                        ws_log_out(&f);
                                        let _ = write.send(Message::Text(f)).await;
                                    }
                                }
                                Err(e) => {
                                    let cid_guess =
                                        serde_json::from_str::<Value>(&txt).ok().and_then(|vv| {
                                            vv.get("cid")
                                                .and_then(|x| x.as_str())
                                                .map(|s| s.to_string())
                                        });
                                    let mut err = api::ErrorResponse::new(
                                        1,
                                        m::error_response::Type::Error,
                                        now_iso(),
                                        e.clone(),
                                    );
                                    err.code = Some("invalid_request".to_string());
                                    err.cid = cid_guess;
                                    let s = serde_json::to_string(&err)
										.unwrap_or_else(|_| "{\"v\":1,\"type\":\"error\",\"server_time\":\"\",\"error\":\"internal\"}".to_string());
                                    ws_log_out(&s);
                                    let _ = write.send(Message::Text(s)).await;
                                }
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        _ => {}
                    }
                }
            }
        });
    }
}

async fn handle_message(text: &str, state: &mut ConnState) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let typ = v
        .get("type")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "missing field `type`".to_string())?;
    let mut out: Vec<String> = Vec::new();
    match typ {
        "list" => {
            let _req: api::ListRequest =
                serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
            let store = state.thread_store();
            let ids = store.list().await;
            let mut threads: Vec<api::ListResponseThreadsInner> = Vec::new();
            for tid in ids {
                let mut item = api::ListResponseThreadsInner::new(tid.clone());
                if let Ok(log) = store.get(&tid).await {
                    let (mut suite_id, agent_type) = derive_thread_context(&log);
                    if suite_id.trim().is_empty() {
                        suite_id = default_suite_id(&state.reg).unwrap_or_default();
                    }
                    // last_activity
                    item.last_activity = log.steps.last().map(|s| s.ts().to_string());
                    item.title = log.title.clone();
                    item.suite_id = Some(suite_id);
                    item.agent_type = Some(agent_type);
                    // compute preview from last user or complete
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
                    // unread count based on seen map vs assistant messages count
                    let seen = state.seen.get(&tid).copied().unwrap_or(0);
                    let (assistant_seq_max, assistant_after_seen) =
                        compute_unread_for_log(&log, seen);
                    let unread = assistant_after_seen;
                    item.unread_count = Some(unread);
                    // ensure thread_seq map is at least assistant_seq_max
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
        }
        "suites" => {
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
        }
        "new" => {
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
            // Persist initial suite/agent selection so it survives reconnects
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
            // Emit a real preflight phase step so UIs get an event during "preflight" (not just a default label).
            {
                let store = state.thread_store();
                if let Ok(Some((step_idx, ts))) =
                    ensure_preflight_phase_step(&store, &thread_id, &agent, Some(&suite_id)).await
                {
                    let runs = vec![api::PhaseRun::new(ts.clone())];
                    let mut ev = api::PhaseResponse::new(
                        1,
                        api::phase_response::Type::Phase,
                        now_iso(),
                        state.next_seq(),
                        thread_id.clone(),
                        step_idx as i32,
                        "preflight".to_string(),
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
            // ok
            let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
            ok.cid = Some(cid.clone());
            out.push(serde_json::to_string(&api::ServerMessage::Ok(ok)).unwrap());
            // thread_assigned
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
            // If a question was provided, record it and run the suite. Otherwise, this is "create thread".
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
                // run suite non-streaming fallback
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
                            let display_text = display
                                .clone()
                                .unwrap_or_else(|| final_display_text_from_payload(&payload));
                            // finalize title once using concise summary
                            {
                                let store = state.thread_store();
                                let title = synthesize_title(
                                    &state.suite_ctx.llm,
                                    &question,
                                    &display_text,
                                )
                                .await;
                                let _ = store.lock_title(&thread_id, &title).await;
                            }
                            // Persist complete so reconnect/history can observe completion.
                            {
                                let store = state.thread_store();
                                append_step_if_new(
                                    &store,
                                    &thread_id,
                                    ThreadStep::Complete {
                                        kind: kind.clone(),
                                        payload: payload.clone(),
                                        display: display.clone(),
                                        observation: Observation::ok(),
                                        ts: chrono::Utc::now().to_rfc3339(),
                                        agent: agent.clone(),
                                    },
                                )
                                .await;
                            }
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
                            // Log entire thread on final
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
                            // persist gate in thread
                            {
                                let store = state.thread_store();
                                append_step_if_new(
                                    &store,
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
                                    prompt.clone(),
                                ));
                            let s = serde_json::to_string(&resp).unwrap();
                            state.buffer_last(&s);
                            out.push(s);
                            // Persist gate so reconnect/history does not stall.
                            {
                                let store = state.thread_store();
                                append_step_if_new(
                                    &store,
                                    &thread_id,
                                    ThreadStep::Interrupt {
                                        kind: "await_approval".to_string(),
                                        prompt,
                                        observation: Observation::ok(),
                                        ts: chrono::Utc::now().to_rfc3339(),
                                        agent: agent.clone(),
                                    },
                                )
                                .await;
                            }
                        }
                    }
                }
            }
        }
        "open" => {
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

            // Derive current suite/agent from persisted thread log (durable across reconnects)
            let store = state.thread_store();
            let log = store.get(&thread_id).await?;
            let (mut current_suite, current_agent) = derive_thread_context(&log);
            if current_suite.trim().is_empty() {
                current_suite = default_suite_id(&state.reg).unwrap_or_default();
            }
            // Track suite per-thread (explicit client selection) and persist switch if changed
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
                // append switch_agent step
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
            // Ensure we always persist the explicitly requested agent for this thread
            state
                .current_agent
                .insert(thread_id.clone(), requested_agent.clone());
            // ok
            let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
            ok.cid = Some(cid.clone());
            out.push(serde_json::to_string(&api::ServerMessage::Ok(ok)).unwrap());
            // Ensure preflight is visible as a first-class phase event before we do any work.
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
                    ensure_preflight_phase_step(&store, &thread_id, &agent, Some(&suite_id)).await
                {
                    let runs = vec![api::PhaseRun::new(ts.clone())];
                    let mut ev = api::PhaseResponse::new(
                        1,
                        api::phase_response::Type::Phase,
                        now_iso(),
                        state.next_seq(),
                        thread_id.clone(),
                        step_idx as i32,
                        "preflight".to_string(),
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
            // run suite non-streaming fallback
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
                        // Log entire thread on final
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
                        // persist gate in thread
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
        }
        "user" => {
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
            // Ensure durable suite/agent state is available after reconnect
            if !state.current_suite.contains_key(&thread_id)
                || !state.current_agent.contains_key(&thread_id)
            {
                let store = state.thread_store();
                let log = store.get(&thread_id).await?;
                let (mut suite_id, agent_type) = derive_thread_context(&log);
                if suite_id.trim().is_empty() {
                    suite_id = default_suite_id(&state.reg).unwrap_or_default();
                }
                state.current_suite.insert(thread_id.clone(), suite_id);
                state.current_agent.insert(thread_id.clone(), agent_type);
            }
            let store = state.thread_store();
            let agent_label = state
                .current_agent
                .get(&thread_id)
                .cloned()
                .unwrap_or_else(|| "ask".to_string());
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
            // ack
            // we don't increment thread_seq on user ack
            let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
            ok.cid = Some(req.cid.clone());
            out.push(serde_json::to_string(&api::ServerMessage::Ok(ok)).unwrap());
            // Steering gates removed: user messages no longer drive model/metric type or existing/new choices.
            // NOTE: For production WS, `type:"user"` is fast-pathed via `process_user` so we can stream progress.
            // This fallback keeps behavior for direct `handle_message` callers but does not stream progress.
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
            // run agent for this thread using the user text
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
                                .unwrap_or_else(|| "ask".to_string());
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
                        // Log entire thread on final (best-effort)
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
        }
        "history" => {
            let req: api::HistoryRequest =
                serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
            let thread_id = req.thread_id.clone();
            if thread_id.is_empty() {
                return Err("thread_id required".into());
            }
            if uuid::Uuid::parse_str(&thread_id).is_err() {
                return Err("invalid thread_id".into());
            }
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
            let log = store.get(&thread_id).await?;
            let (mut suite_id, agent_type) = derive_thread_context(&log);
            if suite_id.trim().is_empty() {
                suite_id = default_suite_id(&state.reg).unwrap_or_default();
            }
            resp.title = log.title;
            resp.suite_id = Some(suite_id);
            resp.agent_type = Some(agent_type);
            resp.next_before_thread_seq = next_before;
            let outm = api::ServerMessage::History(resp);
            let s = serde_json::to_string(&outm).unwrap();
            state.buffer_last(&s);
            out.push(s);
        }
        "seen" => {
            let req: api::SeenRequest =
                serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
            let thread_id = req.thread_id.clone();
            state.seen.insert(thread_id.clone(), req.up_to_thread_seq);
            // compute unread now
            let store = state.thread_store();
            let log = store.get(&thread_id).await?;
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
        }
        "plans" => {
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
        }
        "thread_state" => {
            let req: api::ThreadStateRequest =
                serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
            let thread_id = req.thread_id.clone();
            if thread_id.is_empty() {
                return Err("thread_id required".into());
            }
            if uuid::Uuid::parse_str(&thread_id).is_err() {
                return Err("invalid thread_id".into());
            }

            let store = state.thread_store();
            let st = store.get_thread_state(&thread_id).await?;
            let timeline_events = load_timeline_events(&store, &thread_id).await;
            let snap = ws_thread_state_snapshot_from_core(&st, &timeline_events, state.reg.as_ref());
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
        }
        "delete" => {
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
            // delete thread json
            {
                let store = state.thread_store();
                let _ = store.delete(&thread_id).await;
            }
            // best-effort vector cleanup (optional provider)
            if let Some(vs) = state.suite_ctx.vector.as_ref() {
                let _ = vs
                    .delete_thread_embeddings(&state.suite_ctx.scope, &thread_id)
                    .await;
            }
            // respond ok
            let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
            ok.cid = Some(cid.clone());
            out.push(serde_json::to_string(&api::ServerMessage::Ok(ok)).unwrap());
        }
        _ => {
            return Err("unknown type".to_string());
        }
    }
    Ok(out)
}

/// Headless runner for `react run`.
///
/// It reuses the WS request handlers + streaming loop, but sends frames into a
/// "null" sink (no socket) and emits the same typed `api::ServerMessage` events
/// to the provided `EventHub`.
pub async fn run_headless_with_hub(
    suite_ctx: SuiteCtx,
    thread_id: Option<String>,
    suite_id: String,
    agent: String,
    hub: EventHub,
    registry: SuiteRegistry,
) -> Result<String, String> {
    use serde_json::json;
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    let reg = Arc::new(registry);
    let mut state = ConnState::new(reg, suite_ctx, Some(hub));
    #[derive(Clone, Default)]
    struct Capture {
        thread_id: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    }
    impl Capture {
        fn get(&self) -> Option<String> {
            self.thread_id.lock().ok().and_then(|g| g.clone())
        }
        fn capture(&self, s: &str) {
            if let Ok(v) = serde_json::from_str::<Value>(s) {
                if v.get("type").and_then(|x| x.as_str()) == Some("thread_assigned") {
                    if let Some(tid) = v.get("thread_id").and_then(|x| x.as_str()) {
                        if let Ok(mut g) = self.thread_id.lock() {
                            *g = Some(tid.to_string());
                        }
                    }
                }
            }
        }
    }
    struct CaptureSink {
        cap: Capture,
        _recent: VecDeque<String>,
    }
    impl CaptureSink {
        fn new(cap: Capture) -> Self {
            Self {
                cap,
                _recent: VecDeque::new(),
            }
        }
    }
    impl futures::sink::Sink<Message> for CaptureSink {
        type Error = String;
        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
            if let Message::Text(s) = item {
                self.cap.capture(&s);
                self._recent.push_back(s);
                while self._recent.len() > 32 {
                    self._recent.pop_front();
                }
            }
            Ok(())
        }
        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }
    let cap = Capture::default();
    let mut write = CaptureSink::new(cap.clone());

    if let Some(tid) = thread_id {
        if tid.trim().is_empty() {
            return Err("thread_id required".into());
        }
        if uuid::Uuid::parse_str(&tid).is_err() {
            return Err("invalid thread_id".into());
        }
        // Enforce "must exist" semantics with a clear error message.
        let store = state.thread_store();
        if store.get(&tid).await.is_err() {
            return Err(format!("thread does not exist: {}", tid));
        }

        let v = json!({
            "v": 1,
            "type": "open",
            "cid": "headless",
            "thread_id": tid,
            "suiteId": suite_id,
            "agentType": agent,
            "question": "continue"
        });
        let tid = v
            .get("thread_id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        process_open(&v, &mut state, &mut write).await?;
        return Ok(tid);
    }

    // Create a new thread with an initial "go" prompt.
    let v = json!({
        "v": 1,
        "type": "new",
        "cid": "headless",
        "suiteId": suite_id,
        "agentType": agent,
        "question": "go"
    });
    process_new(&v, &mut state, &mut write).await?;

    cap.get()
        .ok_or_else(|| "internal: failed to capture thread_id from new thread".to_string())
}

async fn process_new(
    v: &Value,
    state: &mut ConnState,
    write: &mut (impl SinkExt<Message> + Unpin),
) -> Result<(), String> {
    let req: api::NewRequest = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let cid = req.cid.clone();
    let question = req.question.clone().unwrap_or_default();
    let thread_id = Uuid::new_v4().to_string();
    let suite_id = req.suite_id.clone();
    let agent = normalize_agent_new(req.agent_type);
    state
        .current_suite
        .insert(thread_id.clone(), suite_id.clone());
    state.current_agent.insert(thread_id.clone(), agent.clone());
    // Persist initial suite/agent selection so it survives reconnects
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
    // ok
    let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
    ok.cid = Some(cid.clone());
    {
        let s = serde_json::to_string(&ok).unwrap();
        ws_log_out(&s);
        let _ = write.send(Message::Text(s)).await;
    }
    // thread_assigned
    let ta = api::ThreadAssignedResponse::new(
        1,
        m::thread_assigned_response::Type::ThreadAssigned,
        now_iso(),
        state.next_seq(),
        cid.clone(),
        thread_id.clone(),
    );
    {
        let s = serde_json::to_string(&ta).unwrap();
        state.buffer_last(&s);
        ws_log_out(&s);
        let _ = write.send(Message::Text(s)).await;
    }
    // agent with periodic updates
    // If a question was provided, record it and run the suite. Otherwise, this is "create thread".
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
        run_suite_and_stream(
            &thread_id,
            &question,
            &suite_id,
            &agent,
            &cid,
            SuiteRunKind::New,
            state,
            write,
        )
        .await?;
    }
    Ok(())
}

async fn process_open(
    v: &Value,
    state: &mut ConnState,
    write: &mut (impl SinkExt<Message> + Unpin),
) -> Result<(), String> {
    let req: api::OpenRequest = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
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

    // Derive current suite/agent from persisted thread log (durable across reconnects)
    let store = state.thread_store();
    let log = store.get(&thread_id).await?;
    let (mut current_suite, current_agent) = derive_thread_context(&log);
    if current_suite.trim().is_empty() {
        current_suite = default_suite_id(&state.reg).unwrap_or_default();
    }
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
    // Ensure we always persist the explicitly requested agent for this thread
    state
        .current_agent
        .insert(thread_id.clone(), requested_agent.clone());
    // ok
    let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
    ok.cid = Some(cid.clone());
    {
        let s = serde_json::to_string(&ok).unwrap();
        ws_log_out(&s);
        let _ = write.send(Message::Text(s)).await;
    }

    // Strongly-consistent materialized state snapshot (durable across reloads).
    {
        let store = state.thread_store();
        let plans = load_latest_plans(
            state.reg.as_ref(),
            &requested_suite,
            &state.suite_ctx,
            &thread_id,
        )
        .await;
        upsert_thread_state_from_plans(&store, &thread_id, &plans).await;
        if let Ok(st) = store.get_thread_state(&thread_id).await {
            let timeline_events = load_timeline_events(&store, &thread_id).await;
            let snap = ws_thread_state_snapshot_from_core(&st, &timeline_events, state.reg.as_ref());
            let mut resp = api::ThreadStateResponse::new(
                1,
                m::thread_state_response::Type::ThreadState,
                now_iso(),
                state.next_seq(),
                thread_id.clone(),
                snap,
            );
            resp.for_cid = Some(cid.clone());
            let s = serde_json::to_string(&resp).unwrap();
            state.buffer_last(&s);
            ws_log_out(&s);
            let _ = write.send(Message::Text(s)).await;
        }
    }
    // agent with periodic updates
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
    // if user supplied a prompt on open, record it
    if !question.trim().is_empty() {
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
    run_suite_and_stream(
        &thread_id,
        &question,
        &suite_id,
        &agent,
        &cid,
        SuiteRunKind::Open,
        state,
        write,
    )
    .await
}

async fn process_user(
    v: &Value,
    state: &mut ConnState,
    write: &mut (impl SinkExt<Message> + Unpin),
) -> Result<(), String> {
    let req: api::UserRequest = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let cid = req.cid.clone();
    let thread_id = req.thread_id.clone();
    let text = req.text.clone();
    if thread_id.is_empty() || text.trim().is_empty() {
        return Err("thread_id and text required".into());
    }
    if uuid::Uuid::parse_str(&thread_id).is_err() {
        return Err("invalid thread_id".into());
    }

    // Reconnect-safe: derive suite/agent from persisted history if not present in connection state
    if !state.current_suite.contains_key(&thread_id)
        || !state.current_agent.contains_key(&thread_id)
    {
        let store = state.thread_store();
        let log = store.get(&thread_id).await?;
        let (mut suite_id, agent_type) = derive_thread_context(&log);
        if suite_id.trim().is_empty() {
            suite_id = default_suite_id(&state.reg).unwrap_or_default();
        }
        state.current_suite.insert(thread_id.clone(), suite_id);
        state.current_agent.insert(thread_id.clone(), agent_type);
    }
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

    // ack
    let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
    ok.cid = Some(cid.clone());
    {
        let s = serde_json::to_string(&ok).unwrap();
        ws_log_out(&s);
        let _ = write.send(Message::Text(s)).await;
    }

    // record user message
    {
        let store = state.thread_store();
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::User {
                    text: text.clone(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: agent.clone(),
                },
            )
            .await;
    }
    run_suite_and_stream(
        &thread_id,
        &text,
        &suite_id,
        &agent,
        &cid,
        SuiteRunKind::User,
        state,
        write,
    )
    .await
}

async fn process_approve(
    v: &Value,
    state: &mut ConnState,
    write: &mut (impl SinkExt<Message> + Unpin),
) -> Result<(), String> {
    let req: api::ApproveRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let cid = req.cid;
    let thread_id = req.thread_id;
    if thread_id.is_empty() {
        return Err("thread_id required".into());
    }
    if uuid::Uuid::parse_str(&thread_id).is_err() {
        return Err("invalid thread_id".into());
    }
    // Reconnect-safe: derive suite/agent from persisted history if not present in connection state
    if !state.current_suite.contains_key(&thread_id)
        || !state.current_agent.contains_key(&thread_id)
    {
        let store = state.thread_store();
        let log = store.get(&thread_id).await?;
        let (mut suite_id, agent_type) = derive_thread_context(&log);
        if suite_id.trim().is_empty() {
            suite_id = default_suite_id(&state.reg).unwrap_or_default();
        }
        state.current_suite.insert(thread_id.clone(), suite_id);
        state.current_agent.insert(thread_id.clone(), agent_type);
    }
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
    // ack
    let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
    ok.cid = Some(cid.clone());
    {
        let s = serde_json::to_string(&ok).unwrap();
        ws_log_out(&s);
        let _ = write.send(Message::Text(s)).await;
    }
    // append user=approve step
    {
        let store = state.thread_store();
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::User {
                    text: "approve".to_string(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: agent.clone(),
                },
            )
            .await;
    }
    tracing::info!("approve: thread_id={} agent={}", thread_id, agent);
    run_suite_and_stream(
        &thread_id,
        "Continue.",
        &suite_id,
        &agent,
        &cid,
        SuiteRunKind::User,
        state,
        write,
    )
    .await
}

async fn process_reject(
    v: &Value,
    state: &mut ConnState,
    write: &mut (impl SinkExt<Message> + Unpin),
) -> Result<(), String> {
    let req: api::RejectRequest =
        serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    let cid = req.cid;
    let thread_id = req.thread_id;
    if thread_id.is_empty() {
        return Err("thread_id required".into());
    }
    if uuid::Uuid::parse_str(&thread_id).is_err() {
        return Err("invalid thread_id".into());
    }
    // Reconnect-safe: derive suite/agent from persisted history if not present in connection state
    if !state.current_suite.contains_key(&thread_id)
        || !state.current_agent.contains_key(&thread_id)
    {
        let store = state.thread_store();
        let log = store.get(&thread_id).await?;
        let (mut suite_id, agent_type) = derive_thread_context(&log);
        if suite_id.trim().is_empty() {
            suite_id = default_suite_id(&state.reg).unwrap_or_default();
        }
        state.current_suite.insert(thread_id.clone(), suite_id);
        state.current_agent.insert(thread_id.clone(), agent_type);
    }
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
    // ack
    let mut ok = api::OkResponse::new(1, m::ok_response::Type::Ok, now_iso());
    ok.cid = Some(cid.clone());
    {
        let s = serde_json::to_string(&ok).unwrap();
        ws_log_out(&s);
        let _ = write.send(Message::Text(s)).await;
    }
    // append user=reject step
    {
        let store = state.thread_store();
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::User {
                    text: "reject".to_string(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: agent.clone(),
                },
            )
            .await;
    }
    tracing::info!("reject: thread_id={} agent={}", thread_id, agent);
    run_suite_and_stream(
        &thread_id,
        "Continue.",
        &suite_id,
        &agent,
        &cid,
        SuiteRunKind::User,
        state,
        write,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{DefaultKeyspace, RequestScope};
    use async_trait::async_trait;
    use futures_util::sink::Sink;
    use react_core::keyspace::Keyspace;
    use react_core::llm::NullModel;
    use react_core::session::{
        ThreadLog, ThreadState as CoreThreadState, ThreadStore, ToolObservation,
    };
    use react_core::providers::NullSecretsProvider;
    use react_core::storage::InMemoryStorageAdapter;
    use serde_json::json;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::task::{Context, Poll};
    use std::time::Duration;

    #[test]
    fn thread_state_snapshot_maps_ctx_from_core_event_field() {
        let mut core = CoreThreadState::default();
        core.thread_state_schema_version = react_core::session::THREAD_STATE_SCHEMA_VERSION;
        core.thread_id = "tid".to_string();
        core.suite_id = Some("suite_x".to_string());
        core.agent_type = Some("agent".to_string());
        core.current_phase = Some("preflight".to_string());
        let timeline_events = vec![react_core::session::ThreadEvent {
            step_idx: 0,
            event_kind: react_core::session::ThreadEventKind::ToolStart,
            ts: "t".to_string(),
            ctx: Some(react_core::session::ExecutionContext {
                plan_kind: Some(react_core::session::ExecutionPlanKind::new("cleanse")),
                plan_key: Some("p1".to_string()),
                workgroup_id: Some("wg1".to_string()),
                task_id: Some("task1".to_string()),
                checklist_item_id: Some("sql_model".to_string()),
                data: std::collections::BTreeMap::from([
                    ("suite".to_string(), serde_json::json!("suite_x")),
                ]),
            }),
            ..Default::default()
        }];
        let reg = react_core::suite::SuiteRegistry::new();
        let snap = ws_thread_state_snapshot_from_core(&core, &timeline_events, &reg);
        let ctx = snap.events[0].ctx.as_ref().expect("ctx");
        assert_eq!(ctx.plan_kind, Some("cleanse".to_string()));
        assert_eq!(ctx.plan_key.as_deref(), Some("p1"));
        assert_eq!(ctx.workgroup_id.as_deref(), Some("wg1"));
        assert_eq!(ctx.task_id.as_deref(), Some("task1"));
        assert_eq!(ctx.checklist_item_id.as_deref(), Some("sql_model"));
    }

    #[derive(Clone, Default)]
    struct CollectSink {
        out: Arc<Mutex<Vec<String>>>,
    }

    impl Sink<Message> for CollectSink {
        type Error = std::convert::Infallible;

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
            if let Message::Text(s) = item {
                self.out.lock().unwrap().push(s);
            }
            Ok(())
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    struct StubDataEngineerSuite;

    #[async_trait]
    impl react_core::suite::Suite for StubDataEngineerSuite {
        fn id(&self) -> &'static str {
            "suite_x"
        }

        fn phase_order(&self, _agent_type: &str) -> Vec<String> {
            vec!["preflight".to_string(), "done".to_string()]
        }

        async fn handle_new(
            &self,
            _thread_id: &str,
            _question: &str,
            _agent_type: &str,
            _ctx: &SuiteCtx,
        ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
            Ok(vec![react_core::suite::FlowFrame::Complete {
                kind: "ask".to_string(),
                payload: serde_json::json!({"answer":"ok","sql":"SELECT 1"}),
                display: Some("ok".to_string()),
            }])
        }

        async fn handle_open(
            &self,
            _thread_id: &str,
            _question: &str,
            _agent_type: &str,
            _ctx: &SuiteCtx,
        ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
            Ok(vec![react_core::suite::FlowFrame::Complete {
                kind: "ask".to_string(),
                payload: serde_json::json!({"answer":"ok","sql":"SELECT 1"}),
                display: Some("ok".to_string()),
            }])
        }

        async fn handle_user(
            &self,
            thread_id: &str,
            _text: &str,
            _agent_type: &str,
            ctx: &SuiteCtx,
        ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
            // Emit a tool_start/tool_end pair into the durable thread log so WS can stream tool events.
            let store =
                ThreadStore::new(ctx.storage.clone(), ctx.scope.clone(), ctx.keyspace.clone());
            let tool_id = "t1".to_string();
            let _ = store
                .append_step(
                    thread_id,
                    ThreadStep::ToolStart {
                        tool_id: tool_id.clone(),
                        name: "dbt_files".to_string(),
                        clean_name: "dbt_files patch".to_string(),
                        args: serde_json::json!({"op":"patch"}),
                        status: react_core::session::ToolStepStatus::Running,
                        payload: Some(serde_json::json!({"hint":"starting"})),
                        ctx: None,
                        ts: chrono::Utc::now().to_rfc3339(),
                        agent: "agent".to_string(),
                    },
                )
                .await;
            let _ = store
                .append_step(
                    thread_id,
                    ThreadStep::ToolEnd {
                        tool_id: tool_id.clone(),
                        name: "dbt_files".to_string(),
                        clean_name: "dbt_files patch".to_string(),
                        args: serde_json::json!({"op":"patch"}),
                        status: react_core::session::ToolStepStatus::Ok,
                        payload: Some(serde_json::json!({"written_keys": []})),
                        ctx: None,
                        observation: ToolObservation::normalize(serde_json::json!({"ok": true})),
                        ts: chrono::Utc::now().to_rfc3339(),
                        agent: "agent".to_string(),
                    },
                )
                .await;

            // Sleep long enough for WS ticks (tool/state) to emit at least once.
            tokio::time::sleep(Duration::from_millis(650)).await;
            Ok(vec![react_core::suite::FlowFrame::Complete {
                kind: "ask".to_string(),
                payload: serde_json::json!({"answer":"ok","sql":"SELECT 1"}),
                display: Some("ok".to_string()),
            }])
        }
    }

    struct StubAwaitApprovalSuite;

    #[async_trait]
    impl react_core::suite::Suite for StubAwaitApprovalSuite {
        fn id(&self) -> &'static str {
            "suite_x"
        }

        fn phase_order(&self, _agent_type: &str) -> Vec<String> {
            vec!["preflight".to_string(), "done".to_string()]
        }

        async fn handle_new(
            &self,
            _thread_id: &str,
            _question: &str,
            _agent_type: &str,
            _ctx: &SuiteCtx,
        ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
            Ok(vec![react_core::suite::FlowFrame::Interrupt {
                kind: "await_approval".to_string(),
                prompt: "approve?".to_string(),
            }])
        }

        async fn handle_open(
            &self,
            _thread_id: &str,
            _question: &str,
            _agent_type: &str,
            _ctx: &SuiteCtx,
        ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
            Ok(vec![react_core::suite::FlowFrame::Interrupt {
                kind: "await_approval".to_string(),
                prompt: "approve?".to_string(),
            }])
        }

        async fn handle_user(
            &self,
            _thread_id: &str,
            _text: &str,
            _agent_type: &str,
            _ctx: &SuiteCtx,
        ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
            Ok(vec![react_core::suite::FlowFrame::Interrupt {
                kind: "await_approval".to_string(),
                prompt: "approve?".to_string(),
            }])
        }
    }

    struct StubBatchLockedSuite;

    #[async_trait]
    impl react_core::suite::Suite for StubBatchLockedSuite {
        fn id(&self) -> &'static str {
            "suite_x"
        }

        fn phase_order(&self, _agent_type: &str) -> Vec<String> {
            vec!["cleanse_author".to_string()]
        }

        async fn handle_new(
            &self,
            _thread_id: &str,
            _question: &str,
            _agent_type: &str,
            _ctx: &SuiteCtx,
        ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
            Ok(vec![react_core::suite::FlowFrame::Interrupt {
                kind: "await_user".to_string(),
                prompt: "Plan-batched authoring is locked (cleanse).".to_string(),
            }])
        }

        async fn handle_open(
            &self,
            _thread_id: &str,
            _question: &str,
            _agent_type: &str,
            _ctx: &SuiteCtx,
        ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
            Ok(vec![react_core::suite::FlowFrame::Interrupt {
                kind: "await_user".to_string(),
                prompt: "Plan-batched authoring is locked (cleanse).".to_string(),
            }])
        }

        async fn handle_user(
            &self,
            _thread_id: &str,
            _text: &str,
            _agent_type: &str,
            _ctx: &SuiteCtx,
        ) -> Result<Vec<react_core::suite::FlowFrame>, String> {
            Ok(vec![react_core::suite::FlowFrame::Interrupt {
                kind: "await_user".to_string(),
                prompt: "Plan-batched authoring is locked (cleanse).".to_string(),
            }])
        }
    }

    #[test]
    fn normalize_agent_includes_agent_and_review() {
        assert_eq!(normalize_agent_new(api::new_request::AgentType::Agent), "agent");
        assert_eq!(normalize_agent_new(api::new_request::AgentType::Review), "review");
        assert_eq!(normalize_agent_open(api::open_request::AgentType::Agent), "agent");
        assert_eq!(normalize_agent_open(api::open_request::AgentType::Review), "review");
    }

    #[test]
    fn derive_thread_context_ignores_step_agent_labels() {
        let log = ThreadLog {
            steps: vec![
                ThreadStep::SwitchSuite {
                    from: None,
                    to: "suite_x".to_string(),
                    observation: Observation::ok(),
                    ts: "t".to_string(),
                    agent: "agent".to_string(),
                },
                ThreadStep::SwitchAgent {
                    from: None,
                    to: "agent".to_string(),
                    observation: Observation::ok(),
                    ts: "t".to_string(),
                    agent: "agent".to_string(),
                },
                // Inner phase/tool steps may record agent labels like "cleanse" — these must NOT
                // override the user-selected agent_type derived from switch_agent.
                ThreadStep::ToolEnd {
                    tool_id: "t".to_string(),
                    name: "sql_schema".to_string(),
                    clean_name: "List tables".to_string(),
                    args: json!({}),
                    status: react_core::session::ToolStepStatus::Ok,
                    payload: None,
                    ctx: None,
                    observation: ToolObservation::normalize(json!({"ok": true})),
                    ts: "t".to_string(),
                    agent: "cleanse".to_string(),
                },
            ],
            ..Default::default()
        };
        let (_suite, agent_type) = derive_thread_context(&log);
        assert_eq!(agent_type, "agent");
    }

    #[test]
    fn unread_counts_include_review_response() {
        let log = ThreadLog {
            steps: vec![
                ThreadStep::User {
                    text: "hi".to_string(),
                    observation: Observation::ok(),
                    ts: "t".to_string(),
                    agent: "ask".to_string(),
                },
                ThreadStep::ReviewResponse {
                    text: "review text".to_string(),
                    meta: None,
                    observation: Observation::ok(),
                    ts: "t".to_string(),
                    agent: "agent".to_string(),
                },
                ThreadStep::Complete {
                    kind: "generic".to_string(),
                    payload: serde_json::json!({ "text": "done" }),
                    display: Some("done".to_string()),
                    observation: Observation::ok(),
                    ts: "t".to_string(),
                    agent: "agent".to_string(),
                },
            ],
            ..Default::default()
        };
        // No seen messages yet -> both assistant messages should count as unread.
        let (_max_seq, unread) = compute_unread_for_log(&log, 0);
        assert_eq!(unread, 2);
    }

    #[tokio::test]
    async fn history_includes_review_response_as_assistant_message() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let store = ThreadStore::new(storage, scope, keyspace);

        let tid = "thread1";
        let _ = store
            .append_step(
                tid,
                ThreadStep::User {
                    text: "start".to_string(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: "ask".to_string(),
                },
            )
            .await;

        let _ = store
            .append_step(
                tid,
                ThreadStep::ReviewResponse {
                    text: "review text".to_string(),
                    meta: None,
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: "agent".to_string(),
                },
            )
            .await;

        let (msgs, _next) = build_history(&store, tid, None, Some(50)).await.unwrap();
        assert!(msgs.iter().any(
            |m| m.role == m::history_response_messages_inner::Role::Assistant
                && m.content == "review text"
        ));
    }

    #[tokio::test]
    async fn suites_request_requires_cid_and_v() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let suite_ctx = SuiteCtx::new(
            storage,
            Arc::new(NullSecretsProvider::default()),
            Arc::new(NullModel::new()),
            scope,
            keyspace,
        );
        let reg = Arc::new(react_suites::default_registry());
        let mut state = ConnState::new(reg, suite_ctx, None);

        // Missing required fields should fail strict parsing.
        let bad = json!({"type":"suites"}).to_string();
        assert!(handle_message(&bad, &mut state).await.is_err());
    }

    #[tokio::test]
    async fn delete_request_requires_cid_and_v() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let suite_ctx = SuiteCtx::new(
            storage,
            Arc::new(NullSecretsProvider::default()),
            Arc::new(NullModel::new()),
            scope,
            keyspace,
        );
        let reg = Arc::new(react_suites::default_registry());
        let mut state = ConnState::new(reg, suite_ctx, None);

        let bad = json!({"type":"delete","thread_id":"not-a-uuid"}).to_string();
        assert!(handle_message(&bad, &mut state).await.is_err());
    }

    #[tokio::test]
    async fn headless_run_requires_existing_thread_when_thread_id_provided() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let suite_ctx = SuiteCtx::new(
            storage,
            Arc::new(NullSecretsProvider::default()),
            Arc::new(NullModel::new()),
            scope,
            keyspace,
        );
        let missing = uuid::Uuid::new_v4().to_string();
        let hub = EventHub::new(256);
        let err = run_headless_with_hub(
            suite_ctx,
            Some(missing.clone()),
            "suite_x".to_string(),
            "agent".to_string(),
            hub,
            SuiteRegistry::new(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("thread does not exist"));
        assert!(err.contains(&missing));
    }

    #[tokio::test]
    async fn headless_run_exits_with_error_on_ask_user_prompt() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let suite_ctx = SuiteCtx::new(
            storage,
            Arc::new(NullSecretsProvider::default()),
            Arc::new(NullModel::new()),
            scope,
            keyspace,
        );

        let mut reg = react_core::suite::SuiteRegistry::new();
        reg.register(StubBatchLockedSuite);
        let reg = Arc::new(reg);
        let mut state = ConnState::new(reg, suite_ctx, None);

        let msg = json!({"v":1,"type":"new","cid":"headless","suiteId":"suite_x","agentType":"agent","question":"go"});
        let mut sink = CollectSink::default();
        let err = process_new(&msg, &mut state, &mut sink).await.unwrap_err();
        assert!(
            err.to_ascii_lowercase()
                .contains("ask_user_not_supported_in_headless")
        );
    }

    #[tokio::test]
    async fn plans_request_returns_latest_cleanse_and_model_when_present() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let suite_ctx = SuiteCtx::new(
            storage.clone(),
            Arc::new(NullSecretsProvider::default()),
            Arc::new(NullModel::new()),
            scope.clone(),
            keyspace.clone(),
        );
        let reg = Arc::new(react_suites::default_registry());
        let mut state = ConnState::new(reg, suite_ctx.clone(), None);

        let thread_id = uuid::Uuid::new_v4().to_string();
        let base = keyspace
            .threads_prefix(&scope)
            .trim_end_matches("/threads")
            .trim_end_matches('/')
            .to_string();

        let cleanse_key = format!("{}/plans/{}/20260126T000000Z_cleanse.json", base, thread_id);
        let cleanse = serde_json::json!({
            "plan_key": cleanse_key,
            "status": "approved",
            "project_snapshot": {},
            "tasks": [],
            "batches": [],
            "work_groups": [],
            "mutations": [],
            "progress": {},
        });
        suite_ctx
            .storage
            .put_bytes(
                &cleanse_key,
                &serde_json::to_vec_pretty(&cleanse).unwrap(),
                "application/json",
            )
            .await
            .unwrap();

        let model_key = format!("{}/plans/{}/20260126T000000Z_model.json", base, thread_id);
        let model = serde_json::json!({
            "plan_key": model_key,
            "status": "approved",
            "project_snapshot": {},
            "tasks": [],
            "batches": [],
            "work_groups": [],
            "mutations": [],
            "progress": {},
        });
        suite_ctx
            .storage
            .put_bytes(
                &model_key,
                &serde_json::to_vec_pretty(&model).unwrap(),
                "application/json",
            )
            .await
            .unwrap();

        let msg = json!({"v":1,"type":"plans","cid":"c1","thread_id":thread_id}).to_string();
        let frames = handle_message(&msg, &mut state).await.unwrap();
        assert_eq!(frames.len(), 1);
        let resp: api::PlansResponse = serde_json::from_str(&frames[0]).unwrap();
        assert_eq!(resp.for_cid.as_deref(), Some("c1"));
        assert_eq!(resp.plans.len(), 2);
    }

    #[tokio::test]
    async fn plans_request_includes_checklist_and_omits_notes() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let suite_ctx = SuiteCtx::new(
            storage.clone(),
            Arc::new(NullSecretsProvider::default()),
            Arc::new(NullModel::new()),
            scope.clone(),
            keyspace.clone(),
        );
        let reg = Arc::new(react_suites::default_registry());
        let mut state = ConnState::new(reg, suite_ctx.clone(), None);

        let thread_id = uuid::Uuid::new_v4().to_string();
        let base = keyspace
            .threads_prefix(&scope)
            .trim_end_matches("/threads")
            .trim_end_matches('/')
            .to_string();

        let cleanse_key = format!("{}/plans/{}/20260126T000000Z_cleanse.json", base, thread_id);
        let cleanse = serde_json::json!({
            "plan_key": cleanse_key,
            "status": "approved",
            "project_snapshot": {},
            "tasks": [{
                "dataset_id": "AwsDataCatalog.test_raw.raw_orders",
                "expected_model_path": "models/staging/stg_test_raw_raw_orders.sql",
                "invariants": [],
                "implementation_spec": {
                    "spec_version": 1,
                    "row_preserving": true,
                    "output_fields": [{
                        "name": "order_id",
                        "kind": "raw",
                        "source_columns": [],
                        "expression": "order_id",
                        "data_type": null,
                        "nullable": false,
                        "description": null
                    }],
                    "prohibited_ops": []
                },
                "status": "pending",
                "checklist": [{
                    "checklist_item_id": "sql_model",
                    "label": "Author staging SQL",
                    "details": null,
                    "status": "pending",
                    "origin": "initial",
                    "evidence": []
                }]
            }],
            "batches": [["AwsDataCatalog.test_raw.raw_orders"]],
            "work_groups": [],
            "mutations": [],
            "progress": {},
        });
        suite_ctx
            .storage
            .put_bytes(
                &cleanse_key,
                &serde_json::to_vec_pretty(&cleanse).unwrap(),
                "application/json",
            )
            .await
            .unwrap();

        let msg = json!({"v":1,"type":"plans","cid":"c1","thread_id":thread_id}).to_string();
        let frames = handle_message(&msg, &mut state).await.unwrap();
        assert_eq!(frames.len(), 1);

        let v: serde_json::Value = serde_json::from_str(&frames[0]).unwrap();
        let cleanse_snap = v
            .get("plans")
            .and_then(|x| x.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find(|p| p.get("planKind").and_then(|k| k.as_str()) == Some("cleanse"))
            })
            .expect("cleanse plan");
        let tasks = cleanse_snap
            .get("tasks")
            .and_then(|x| x.as_array())
            .unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(
            tasks[0].get("taskKind").and_then(|x| x.as_str()),
            Some("cleanse")
        );
        let cl = tasks[0]
            .get("checklist")
            .and_then(|x| x.as_array())
            .unwrap();
        assert_eq!(
            cl[0].get("checklistItemId").and_then(|x| x.as_str()),
            Some("sql_model")
        );
        assert!(tasks[0].get("notes").is_none());
    }

    #[tokio::test]
    async fn plans_request_surfaces_parse_error_snapshot_for_corrupt_plan_json() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let suite_ctx = SuiteCtx::new(
            storage.clone(),
            Arc::new(NullSecretsProvider::default()),
            Arc::new(NullModel::new()),
            scope.clone(),
            keyspace.clone(),
        );
        let reg = Arc::new(react_suites::default_registry());
        let mut state = ConnState::new(reg, suite_ctx.clone(), None);

        let thread_id = uuid::Uuid::new_v4().to_string();
        let base = keyspace
            .threads_prefix(&scope)
            .trim_end_matches("/threads")
            .trim_end_matches('/')
            .to_string();
        let cleanse_key = format!("{}/plans/{}/20260126T000000Z_cleanse.json", base, thread_id);
        suite_ctx
            .storage
            .put_bytes(&cleanse_key, b"{not valid json", "application/json")
            .await
            .unwrap();

        let msg = json!({"v":1,"type":"plans","cid":"c1","thread_id":thread_id}).to_string();
        let frames = handle_message(&msg, &mut state).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&frames[0]).unwrap();
        let cleanse_snap = v
            .get("plans")
            .and_then(|x| x.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find(|p| p.get("planKind").and_then(|k| k.as_str()) == Some("cleanse"))
            })
            .expect("cleanse plan");
        assert_eq!(
            cleanse_snap.get("status").and_then(|x| x.as_str()),
            Some("cancelled")
        );
        let tasks = cleanse_snap
            .get("tasks")
            .and_then(|x| x.as_array())
            .unwrap();
        let cl = tasks[0]
            .get("checklist")
            .and_then(|x| x.as_array())
            .unwrap();
        assert_eq!(
            cl[0].get("checklistItemId").and_then(|x| x.as_str()),
            Some("parse_error")
        );
        let ps = cleanse_snap
            .get("projectSnapshot")
            .and_then(|x| x.as_object())
            .expect("projectSnapshot");
        assert!(ps.get("parse_error").is_some());
    }

    #[tokio::test]
    async fn open_emits_thread_state_frame() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let suite_ctx = SuiteCtx::new(
            storage.clone(),
            Arc::new(NullSecretsProvider::default()),
            Arc::new(NullModel::new()),
            scope.clone(),
            keyspace.clone(),
        );
        let mut reg = react_core::suite::SuiteRegistry::new();
        reg.register(StubDataEngineerSuite);
        let reg = Arc::new(reg);
        let mut state = ConnState::new(reg, suite_ctx.clone(), None);

        // Seed empty thread log so process_open can load it.
        let thread_id = uuid::Uuid::new_v4().to_string();
        let store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
        // minimal steps so derive_thread_context has something (optional)
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::SwitchSuite {
                    from: None,
                    to: "suite_x".to_string(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: "agent".to_string(),
                },
            )
            .await;

        let msg = json!({"v":1,"type":"open","cid":"c1","thread_id":thread_id,"suiteId":"suite_x","agentType":"agent","question":"Continue."});
        let mut sink = CollectSink::default();
        process_open(&msg, &mut state, &mut sink).await.unwrap();

        let frames = sink.out.lock().unwrap().clone();
        assert!(frames.iter().any(|s| {
            serde_json::from_str::<serde_json::Value>(s)
                .ok()
                .and_then(|v| {
                    v.get("type")
                        .and_then(|t| t.as_str())
                        .map(|t| t == "thread_state")
                })
                .unwrap_or(false)
        }));
    }

    #[tokio::test]
    async fn open_persists_final_step_in_thread_log() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let suite_ctx = SuiteCtx::new(
            storage.clone(),
            Arc::new(NullSecretsProvider::default()),
            Arc::new(NullModel::new()),
            scope.clone(),
            keyspace.clone(),
        );
        let mut reg = react_core::suite::SuiteRegistry::new();
        reg.register(StubDataEngineerSuite);
        let reg = Arc::new(reg);
        let mut state = ConnState::new(reg, suite_ctx.clone(), None);

        let thread_id = uuid::Uuid::new_v4().to_string();
        let store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::SwitchSuite {
                    from: None,
                    to: "suite_x".to_string(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: "agent".to_string(),
                },
            )
            .await;

        let msg = json!({"v":1,"type":"open","cid":"c1","thread_id":thread_id,"suiteId":"suite_x","agentType":"agent","question":"Continue."});
        let mut sink = CollectSink::default();
        process_open(&msg, &mut state, &mut sink).await.unwrap();

        let log = store.get(&thread_id).await.unwrap();
        assert!(matches!(log.steps.last(), Some(ThreadStep::Complete { .. })));
    }

    #[tokio::test]
    async fn open_persists_await_approval_step_in_thread_log() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let suite_ctx = SuiteCtx::new(
            storage.clone(),
            Arc::new(NullSecretsProvider::default()),
            Arc::new(NullModel::new()),
            scope.clone(),
            keyspace.clone(),
        );
        let mut reg = react_core::suite::SuiteRegistry::new();
        reg.register(StubAwaitApprovalSuite);
        let reg = Arc::new(reg);
        let mut state = ConnState::new(reg, suite_ctx.clone(), None);

        let thread_id = uuid::Uuid::new_v4().to_string();
        let store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::SwitchSuite {
                    from: None,
                    to: "suite_x".to_string(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: "agent".to_string(),
                },
            )
            .await;

        let msg = json!({"v":1,"type":"open","cid":"c1","thread_id":thread_id,"suiteId":"suite_x","agentType":"agent","question":"Continue."});
        let mut sink = CollectSink::default();
        process_open(&msg, &mut state, &mut sink).await.unwrap();

        let log = store.get(&thread_id).await.unwrap();
        assert!(matches!(
            log.steps.last(),
            Some(ThreadStep::Interrupt { .. })
        ));
    }

    #[tokio::test]
    async fn user_request_streams_thread_state_and_tool_events() {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        let suite_ctx = SuiteCtx::new(
            storage.clone(),
            Arc::new(NullSecretsProvider::default()),
            Arc::new(NullModel::new()),
            scope.clone(),
            keyspace.clone(),
        );

        // Seed a thread with durable suite/agent selection so process_user can derive context.
        let thread_id = uuid::Uuid::new_v4().to_string();
        let store = ThreadStore::new(storage.clone(), scope.clone(), keyspace.clone());
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::SwitchSuite {
                    from: None,
                    to: "suite_x".to_string(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: "agent".to_string(),
                },
            )
            .await;
        let _ = store
            .append_step(
                &thread_id,
                ThreadStep::SwitchAgent {
                    from: None,
                    to: "agent".to_string(),
                    observation: Observation::ok(),
                    ts: chrono::Utc::now().to_rfc3339(),
                    agent: "agent".to_string(),
                },
            )
            .await;

        let mut reg = react_core::suite::SuiteRegistry::new();
        reg.register(StubDataEngineerSuite);
        let reg = Arc::new(reg);
        let mut state = ConnState::new(reg, suite_ctx, None);

        let msg = json!({"v":1,"type":"user","cid":"c1","thread_id":thread_id,"text":"continue"});
        let mut sink = CollectSink::default();
        process_user(&msg, &mut state, &mut sink).await.unwrap();

        let frames = sink.out.lock().unwrap().clone();
        assert!(!frames.is_empty());
        let mut saw_thread_state = false;
        let mut saw_tool_start = false;
        let mut saw_tool_end = false;
        let mut saw_tool_phase = false;
        for s in frames {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "thread_state" => saw_thread_state = true,
                    "tool_start" => saw_tool_start = true,
                    "tool_end" => saw_tool_end = true,
                    _ => {}
                }
                if matches!(
                    v.get("type").and_then(|t| t.as_str()),
                    Some("tool_start" | "tool_end")
                ) {
                    if v.get("phase")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .trim()
                        .is_empty()
                        == false
                    {
                        saw_tool_phase = true;
                    }
                }
            }
        }
        assert!(saw_thread_state);
        assert!(saw_tool_start);
        assert!(saw_tool_end);
        assert!(saw_tool_phase);
    }
}
