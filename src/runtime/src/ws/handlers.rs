use futures_util::SinkExt;
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

use crate::models as m;
use crate::ws::api_gen::src::models as api;
use react_core::session::{Observation, ThreadStep};
use uuid::Uuid;

use super::conn_state::{
    default_suite_id, derive_thread_context, load_latest_plans, normalize_agent_new,
    normalize_agent_open, ConnState,
};
use super::suite_runner::{run_suite_and_stream, SuiteRunKind};
use super::thread_state::{
    load_timeline_events, upsert_thread_state_from_plans, ws_thread_state_snapshot_from_core,
};
use super::util::{now_iso, truncate_title, ws_log_out};

pub(super) async fn process_new(
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
        let s = serde_json::to_string(&ok).map_err(|e| format!("JSON serialization failed: {e}"))?;
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
        let s = serde_json::to_string(&ta).map_err(|e| format!("JSON serialization failed: {e}"))?;
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

pub(super) async fn process_open(
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
    let log = store.get(&thread_id).await.map_err(|e| e.to_string())?;
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
        let s = serde_json::to_string(&ok).map_err(|e| format!("JSON serialization failed: {e}"))?;
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
            let s = serde_json::to_string(&resp).map_err(|e| format!("JSON serialization failed: {e}"))?;
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

pub(super) async fn process_user(
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
        let log = store.get(&thread_id).await.map_err(|e| e.to_string())?;
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
        let s = serde_json::to_string(&ok).map_err(|e| format!("JSON serialization failed: {e}"))?;
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

pub(super) async fn process_approve(
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
        let log = store.get(&thread_id).await.map_err(|e| e.to_string())?;
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
        let s = serde_json::to_string(&ok).map_err(|e| format!("JSON serialization failed: {e}"))?;
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

pub(super) async fn process_reject(
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
        let log = store.get(&thread_id).await.map_err(|e| e.to_string())?;
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
        let s = serde_json::to_string(&ok).map_err(|e| format!("JSON serialization failed: {e}"))?;
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
