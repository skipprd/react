use serde_json::Value;
use chrono::Utc;

pub(super) fn env_bool(key: &str, default: bool) -> bool {
    let dv = if default { "1" } else { "0" };
    match std::env::var(key).unwrap_or_else(|_| dv.to_string()).trim() {
        "1" | "true" | "TRUE" | "yes" | "YES" => true,
        "0" | "false" | "FALSE" | "no" | "NO" => false,
        _ => default,
    }
}

pub(super) fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| {
            let vv = v.trim().to_lowercase();
            vv == "1" || vv == "true" || vv == "yes"
        })
        .unwrap_or(false)
}

pub(super) fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub(super) fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    match s.char_indices().take_while(|(i, _)| *i < max).last() {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.chars().take(max).collect(),
    }
}

pub(super) fn truncate_title(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        return s.to_string();
    }
    match s.char_indices().take_while(|(i, _)| *i < max_chars).last() {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.chars().take(max_chars).collect(),
    }
}

pub(super) fn ws_log_in(txt: &str) {
    if env_bool("WS_LOG_BODIES", false) {
        tracing::trace!("WS <- {}", txt);
        return;
    }
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    if let Ok(v) = serde_json::from_str::<Value>(txt) {
        let typ = v.get("type").and_then(|x| x.as_str()).unwrap_or("unknown");
        let cid = v.get("cid").and_then(|x| x.as_str());
        let thread_id = v.get("thread_id").and_then(|x| x.as_str());
        tracing::debug!(
            "WS <- type={} cid={} thread_id={}",
            typ,
            cid.unwrap_or("-"),
            thread_id.unwrap_or("-")
        );
    } else {
        tracing::debug!("WS <- (non-json) bytes={}", txt.len());
    }
}

pub(super) fn ws_log_out(txt: &str) {
    if env_bool("WS_LOG_BODIES", false) {
        tracing::trace!("WS -> {}", txt);
        return;
    }
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    if let Ok(v) = serde_json::from_str::<Value>(txt) {
        let typ = v.get("type").and_then(|x| x.as_str()).unwrap_or("unknown");
        let seq = v.get("seq").and_then(|x| x.as_i64());
        let cid = v.get("cid").and_then(|x| x.as_str());
        let for_cid = v.get("for_cid").and_then(|x| x.as_str());
        let thread_id = v.get("thread_id").and_then(|x| x.as_str());
        tracing::debug!(
            "WS -> type={} seq={} cid={} for_cid={} thread_id={}",
            typ,
            seq.map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string()),
            cid.unwrap_or("-"),
            for_cid.unwrap_or("-"),
            thread_id.unwrap_or("-")
        );
    } else {
        tracing::debug!("WS -> (non-json) bytes={}", txt.len());
    }
}
