use serde_json::Value;
use crate::ws::api_gen::src::models as api;

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

pub(super) fn ws_final_result_from_typed_final(
    kind: &str,
    payload: &Value,
    display: &Option<String>,
) -> api::FinalResult {
    match kind {
        "ask" => {
            let answer = payload
                .get("answer")
                .and_then(|x| x.as_str())
                .or_else(|| display.as_deref())
                .unwrap_or("")
                .to_string();
            let sql = payload
                .get("sql")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if answer.trim().is_empty() || sql.trim().is_empty() {
                return ws_final_result_from_typed_final("generic", payload, display);
            }
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
        }
        "kb" => {
            let answer = payload
                .get("answer")
                .and_then(|x| x.as_str())
                .or_else(|| display.as_deref())
                .unwrap_or("")
                .to_string();
            if answer.trim().is_empty() {
                return ws_final_result_from_typed_final("generic", payload, display);
            }
            let p = api::KbFinalPayload::new(answer);
            api::FinalResult::Kb(api::KbFinalResult::new(api::kb_final_result::Kind::Kb, p))
        }
        _ => {
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
}

pub(super) fn map_plan_kind(plan_kind: Option<react_core::session::ExecutionPlanKind>) -> Option<String> {
    plan_kind.map(|k| k.0)
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
    out.plan_kind = map_plan_kind(c.plan_kind.clone());
    out.plan_key = c.plan_key.clone();
    out.workgroup_id = c.workgroup_id.clone();
    out.task_id = c.task_id.clone();
    out.checklist_item_id = c.checklist_item_id.clone();
    out
}
