use super::mapping::final_display_text_from_payload;
use crate::models as m;
use crate::ws::api_gen::src::models as api;
use react_core::session::{ThreadLog, ThreadLogReader, ThreadStep};

pub(super) fn compute_unread_for_log(log: &ThreadLog, seen_seq: i32) -> (i32, i32) {
    let mut tseq: i32 = 0;
    let mut assistant_count_after_seen: i32 = 0;
    let mut last_assistant_seq: i32 = 0;
    for step in log.steps.iter() {
        match step {
            ThreadStep::User { .. } => {
                tseq += 1;
            }
            ThreadStep::Complete { .. }
            | ThreadStep::Interrupt { .. }
            | ThreadStep::ReviewResponse { .. } => {
                tseq += 1;
                if tseq > seen_seq {
                    assistant_count_after_seen += 1;
                }
                last_assistant_seq = tseq;
            }
            _ => {}
        }
    }
    (last_assistant_seq, assistant_count_after_seen)
}

pub(super) async fn build_history(
    reader: &(impl ThreadLogReader + ?Sized),
    thread_id: &str,
    before: Option<i32>,
    limit_opt: Option<i32>,
) -> Result<(Vec<api::HistoryResponseMessagesInner>, Option<i32>), String> {
    let limit = limit_opt.unwrap_or(50).max(1);
    let log = reader.get_log(thread_id).await.map_err(|e| e.to_string())?;
    let mut tseq: i32 = 0;
    let mut all_msgs: Vec<api::HistoryResponseMessagesInner> = Vec::new();
    for step in log.steps.iter() {
        match step {
            ThreadStep::User { text, .. } => {
                tseq += 1;
                all_msgs.push(api::HistoryResponseMessagesInner {
                    thread_seq: tseq,
                    role: m::history_response_messages_inner::Role::User,
                    content: text.to_string(),
                    created_at: step.ts().to_string(),
                });
            }
            ThreadStep::Complete {
                payload, display, ..
            } => {
                tseq += 1;
                let content = display
                    .clone()
                    .or_else(|| {
                        payload
                            .get("text")
                            .and_then(|x| x.as_str())
                            .map(|s| s.to_string())
                    })
                    .or_else(|| {
                        payload
                            .get("answer")
                            .and_then(|x| x.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| final_display_text_from_payload(payload));
                all_msgs.push(api::HistoryResponseMessagesInner {
                    thread_seq: tseq,
                    role: m::history_response_messages_inner::Role::Assistant,
                    content,
                    created_at: step.ts().to_string(),
                });
            }
            ThreadStep::ReviewResponse { text, .. } => {
                tseq += 1;
                all_msgs.push(api::HistoryResponseMessagesInner {
                    thread_seq: tseq,
                    role: m::history_response_messages_inner::Role::Assistant,
                    content: text.to_string(),
                    created_at: step.ts().to_string(),
                });
            }
            ThreadStep::Interrupt { prompt, .. } => {
                tseq += 1;
                all_msgs.push(api::HistoryResponseMessagesInner {
                    thread_seq: tseq,
                    role: m::history_response_messages_inner::Role::Assistant,
                    content: prompt.to_string(),
                    created_at: step.ts().to_string(),
                });
            }
            _ => {}
        }
    }
    let mut filtered: Vec<api::HistoryResponseMessagesInner> = if let Some(b) = before {
        all_msgs.into_iter().filter(|m| m.thread_seq < b).collect()
    } else {
        all_msgs
    };
    let total = filtered.len() as i32;
    let (msgs, next_before) = if total > limit {
        let start = (total - limit) as usize;
        let trimmed = filtered.split_off(start);
        let first_seq = trimmed.first().map(|m| m.thread_seq).unwrap_or(0);
        let nb = if first_seq > 1 { Some(first_seq) } else { None };
        (trimmed, nb)
    } else {
        (filtered, None)
    };
    Ok((msgs, next_before))
}
