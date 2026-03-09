use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use crate::models as m;
use crate::event_hub::EventHub;
use crate::ws::api_gen::src::models as api;
use crate::ws::terminal::{self, TerminalEvent};
use react_core::suite::{SuiteCtx, SuiteRegistry};
use std::sync::Arc;

use super::conn_state::ConnState;
use super::handlers::*;
use super::protocol::*;
use super::util::{now_iso, ws_log_in, ws_log_out};

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

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
