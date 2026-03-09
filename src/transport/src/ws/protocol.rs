use serde_json::Value;

use crate::models as m;
use crate::ws::api_gen::src::models as api;
use react_core::session::{ThreadLogReader, ThreadStep};

use super::conn_state::{
    build_suites_catalog, default_suite_id, load_latest_plans,
    resolve_suite_id_for_thread, resolve_thread_context, ConnState,
};
use super::history::{build_history, compute_unread_for_log};
use super::mapping::final_display_text_from_payload;
use super::terminal::TerminalEvent;
use super::thread_state::{
    load_materialized_state, load_timeline_events, ws_thread_state_snapshot_from_core,
};
use super::util::now_iso;

pub(super) async fn handle_message(text: &str, state: &mut ConnState) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let typ = v
        .get("type")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "missing field `type`".to_string())?;
    match typ {
        "list" => handle_list_message(&v, state).await,
        "suites" => handle_suites_message(&v, state).await,
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
        let reader = state.log_reader();
        reader.list_thread_ids().await
    };
    let mut threads: Vec<api::ListResponseThreadsInner> = Vec::new();
    for tid in ids {
        let mut item = api::ListResponseThreadsInner::new(tid.clone());
        let (suite_id, agent_type) = resolve_thread_context(state, &tid).await;
        let reader = state.log_reader();
        if let Ok(log) = reader.get_log(&tid).await {
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
    let reader = state.log_reader();
    let (messages, next_before) =
        build_history(&reader, &thread_id, req.before_thread_seq, req.limit).await?;
    let mut resp = api::HistoryResponse::new(
        1,
        m::history_response::Type::History,
        now_iso(),
        state.next_seq(),
        thread_id.clone(),
        messages,
    );
    let log = reader.get_log(&thread_id).await.map_err(|e| e.to_string())?;
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
    let reader = state.log_reader();
    let log = reader.get_log(&thread_id).await.map_err(|e| e.to_string())?;
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
    let reader = state.log_reader();
    let st = load_materialized_state(&reader, &thread_id)
        .await
        .ok_or_else(|| "thread not found".to_string())?;
    let timeline_events = load_timeline_events(&reader, &thread_id).await;
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
        let writer = state.log_writer();
        let _ = writer.delete(&thread_id).await;
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

