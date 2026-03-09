use serde_json::Value;
use crate::ws::api_gen::src::models as api;

// ── Naming bridge: core "Complete" ↔ WS API "Final" ─────────────────
//
// The core domain model uses `ThreadStep::Complete` / `FlowFrame::Complete` for
// the terminal agent output, while the WS OpenAPI spec uses the term "Final"
// (`FinalResponse`, `FinalResult`, etc.).  The two terms are synonymous.
//
// Type aliases below let call-sites that prefer the domain name use
// `CompleteResult` etc. without touching the generated code.  A full rename of
// the OpenAPI schema is tracked but deferred to avoid a large coordinated
// change across ~30+ generated files.

#[allow(dead_code)]
pub(super) type CompleteResult = api::FinalResult;
#[allow(dead_code)]
pub(super) type CompleteResponse = api::FinalResponse;
#[allow(dead_code)]
pub(super) type AskCompletePayload = api::AskFinalPayload;
#[allow(dead_code)]
pub(super) type AskCompleteResult = api::AskFinalResult;
#[allow(dead_code)]
pub(super) type KbCompletePayload = api::KbFinalPayload;
#[allow(dead_code)]
pub(super) type KbCompleteResult = api::KbFinalResult;
#[allow(dead_code)]
pub(super) type GenericCompletePayload = api::GenericFinalPayload;
#[allow(dead_code)]
pub(super) type GenericCompleteResult = api::GenericFinalResult;

pub(super) fn final_display_text_from_payload(payload: &Value) -> String {
    if let Some(s) = payload.get("answer").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(s) = payload.get("text").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Ok(s) = serde_json::to_string(payload) {
        return s;
    }
    String::new()
}

/// Build the WS API `FinalResult` (a.k.a. "Complete" in core) by inspecting
/// the payload structure rather than matching on the `kind` string.  This keeps
/// the mapping layer agnostic of suite-defined kind names.
///
/// "Final" is the WS API term; "Complete" is the core domain term — they are
/// synonymous.  See the type aliases at the top of this module.
pub(super) fn ws_final_result_from_typed_final(
    _kind: &str,
    payload: &Value,
    display: &Option<String>,
) -> api::FinalResult {
    let has_answer = payload
        .get("answer")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let has_sql = payload
        .get("sql")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    if has_answer && has_sql {
        let answer = payload["answer"].as_str().unwrap().to_string();
        let sql = payload["sql"].as_str().unwrap().to_string();
        let mut p = api::AskFinalPayload::new(answer, sql);
        if let Some(v) = payload.get("data").cloned() {
            if let Ok(d) = serde_json::from_value::<api::AskFinalPayloadData>(v) {
                p.data = Some(d);
            }
        }
        if let Some(v) = payload.get("chart").cloned() {
            if let Ok(c) = serde_json::from_value::<api::AskFinalPayloadChart>(v) {
                p.chart = Some(c);
            }
        }
        api::FinalResult::Ask(api::AskFinalResult::new(
            api::ask_final_result::Kind::Ask,
            p,
        ))
    } else if has_answer {
        let answer = payload["answer"].as_str().unwrap().to_string();
        let p = api::KbFinalPayload::new(answer);
        api::FinalResult::Kb(api::KbFinalResult::new(api::kb_final_result::Kind::Kb, p))
    } else {
        let txt = payload
            .get("text")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .or_else(|| display.clone())
            .unwrap_or_else(|| final_display_text_from_payload(payload));
        let p = api::GenericFinalPayload::new(txt);
        api::FinalResult::Generic(api::GenericFinalResult::new(
            api::generic_final_result::Kind::Generic,
            p,
        ))
    }
}

pub(super) fn map_thread_event_kind(event_kind: react_core::session::ThreadEventKind) -> api::ThreadEventKind {
    match event_kind {
        react_core::session::ThreadEventKind::ToolStart => api::ThreadEventKind::ToolStart,
        react_core::session::ThreadEventKind::ToolEnd => api::ThreadEventKind::ToolEnd,
        react_core::session::ThreadEventKind::LlmStart => api::ThreadEventKind::LlmStart,
        react_core::session::ThreadEventKind::LlmEnd => api::ThreadEventKind::LlmEnd,
    }
}

pub(super) fn map_tool_event_status(status: Option<react_core::session::ThreadEventStatus>) -> api::ToolEventStatus {
    match status {
        Some(react_core::session::ThreadEventStatus::Running) => api::ToolEventStatus::Running,
        Some(react_core::session::ThreadEventStatus::Ok) => api::ToolEventStatus::Ok,
        Some(react_core::session::ThreadEventStatus::Failed) => api::ToolEventStatus::Failed,
        None => api::ToolEventStatus::Ok,
    }
}

pub(super) fn map_exec_ctx(c: &react_core::session::ExecutionContext) -> api::ExecutionContext {
    let mut out = api::ExecutionContext::new();
    out.plan_kind = c.get_str("plan_kind").map(|s| s.to_string());
    out.plan_key = c.get_str("plan_key").map(|s| s.to_string());
    out.workgroup_id = c.get_str("workgroup_id").map(|s| s.to_string());
    out.task_id = c.get_str("task_id").map(|s| s.to_string());
    out.checklist_item_id = c.get_str("checklist_item_id").map(|s| s.to_string());
    out
}
