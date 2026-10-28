//! Typed agent transcript. Roles are `ChatRole`; prefix strings are illegal.

use std::num::NonZeroU32;

use crate::llm::{ChatMessage, ChatRole};
use crate::text::truncate_chars;
use crate::tools::ToolOutcome;

/// Token ceilings for one prompt render. Derived from model context, not env.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PromptBudget {
    pub total_tokens: NonZeroU32,
    pub per_entry_tokens: NonZeroU32,
}

impl PromptBudget {
    pub const DEFAULT_CONTEXT: u32 = 8_192;
    pub const DEFAULT_MAX_OUTPUT: u32 = 4_096;

    pub fn from_model(context_length: u32, max_output: u32) -> Self {
        let ctx = context_length.max(1_024);
        let reserved = max_output.min(ctx.saturating_sub(512)).max(1);
        let total = ctx.saturating_sub(reserved).max(1_024);
        let per_entry = (total / 8).max(256);
        Self {
            total_tokens: NonZeroU32::new(total).expect("total >= 1024"),
            per_entry_tokens: NonZeroU32::new(per_entry).expect("per_entry >= 256"),
        }
    }

    pub fn from_resolved(llm: &crate::resolved_config::LlmResolved) -> Self {
        Self::from_model(
            llm.context_length.unwrap_or(Self::DEFAULT_CONTEXT as usize) as u32,
            llm.max_tokens.unwrap_or(Self::DEFAULT_MAX_OUTPUT),
        )
    }

    pub fn per_entry_chars(self) -> usize {
        tokens_to_chars(self.per_entry_tokens.get())
    }

    pub fn total_chars(self) -> usize {
        tokens_to_chars(self.total_tokens.get())
    }
}

const LEDGER_CAP: usize = 8;
const LEDGER_EXCERPT_CHARS: usize = 400;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgerEntry {
    pub fingerprint: String,
    pub count: u16,
    pub excerpt: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrorLedger {
    entries: Vec<LedgerEntry>,
}

impl ErrorLedger {
    pub fn record(&mut self, fingerprint: impl Into<String>, excerpt: impl Into<String>) {
        let fingerprint = fingerprint.into();
        let excerpt = truncate_chars(&excerpt.into(), LEDGER_EXCERPT_CHARS);
        if let Some(idx) = self
            .entries
            .iter()
            .position(|e| e.fingerprint == fingerprint)
        {
            let mut e = self.entries.remove(idx);
            e.count = e.count.saturating_add(1);
            e.excerpt = excerpt;
            self.entries.push(e);
            return;
        }
        if self.entries.len() >= LEDGER_CAP {
            self.entries.remove(0);
        }
        self.entries.push(LedgerEntry {
            fingerprint,
            count: 1,
            excerpt,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn render(&self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        let mut out = String::from("Known failures:");
        for e in &self.entries {
            out.push_str(&format!(
                "\n- {} ({}x): {}",
                e.fingerprint, e.count, e.excerpt
            ));
        }
        Some(out)
    }
}

#[derive(Clone, Debug)]
pub enum Entry {
    System(String),
    ToolCard(String),
    OutputContract(String),
    Prelude(String),
    User(String),
    Assistant(String),
    Observation { tool: String, outcome: ToolOutcome },
    ParseRejected(String),
    CompletionRejected { reason: String },
}

impl Entry {
    fn role(&self) -> ChatRole {
        match self {
            Self::System(_) | Self::ToolCard(_) | Self::OutputContract(_) | Self::Prelude(_) => {
                ChatRole::System
            }
            Self::User(_)
            | Self::Observation { .. }
            | Self::ParseRejected(_)
            | Self::CompletionRejected { .. } => ChatRole::User,
            Self::Assistant(_) => ChatRole::Assistant,
        }
    }

    fn content(&self, per_entry_chars: usize) -> String {
        let raw = match self {
            Self::System(s) | Self::Prelude(s) | Self::User(s) | Self::Assistant(s) => s.clone(),
            Self::ToolCard(s) => s.clone(),
            Self::OutputContract(s) => s.clone(),
            Self::Observation { tool, outcome } => match outcome {
                ToolOutcome::Ok(v) => format!("{tool} -> {v}"),
                ToolOutcome::Failed(err) => format!("{tool} failed: {}", err.message),
            },
            Self::ParseRejected(s) => format!("parse rejected: {s}"),
            Self::CompletionRejected { reason } => format!("completion rejected: {reason}"),
        };
        truncate_entry(&raw, per_entry_chars)
    }

    pub fn trace(&self) -> String {
        match self {
            Self::System(s) => format!("system {s}"),
            Self::ToolCard(s) => format!("tools {s}"),
            Self::OutputContract(s) => format!("contract {s}"),
            Self::Prelude(s) => format!("prelude {s}"),
            Self::User(s) => format!("user {s}"),
            Self::Assistant(s) => format!("assistant {s}"),
            Self::Observation { tool, outcome } => match outcome {
                ToolOutcome::Ok(_) => format!("observation {tool} ok"),
                ToolOutcome::Failed(_) => format!("observation {tool} failed"),
            },
            Self::ParseRejected(s) => format!("parse_rejected {s}"),
            Self::CompletionRejected { reason } => format!("completion_rejected {reason}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Transcript {
    head: Vec<Entry>,
    tail: Vec<Entry>,
    ledger: ErrorLedger,
}

impl Transcript {
    pub fn start(
        system: impl Into<String>,
        tools_card: impl Into<String>,
        output_contract: impl Into<String>,
        prelude: Vec<Entry>,
        question: impl Into<String>,
    ) -> Self {
        let mut head = vec![
            Entry::System(system.into()),
            Entry::ToolCard(tools_card.into()),
            Entry::OutputContract(output_contract.into()),
        ];
        for e in prelude {
            match e {
                Entry::Prelude(_) => head.push(e),
                other => head.push(Entry::Prelude(other.content(usize::MAX))),
            }
        }
        head.push(Entry::User(question.into()));
        Self {
            head,
            tail: Vec::new(),
            ledger: ErrorLedger::default(),
        }
    }

    pub fn push(&mut self, e: Entry) {
        self.tail.push(e);
    }

    pub fn record_error(&mut self, fingerprint: impl Into<String>, excerpt: impl Into<String>) {
        self.ledger.record(fingerprint, excerpt);
    }

    pub fn ledger(&self) -> &ErrorLedger {
        &self.ledger
    }

    /// Render into real chat roles. Head is never dropped. Oldest tail drops first.
    pub fn render(&self, budget: PromptBudget) -> Vec<ChatMessage> {
        let per_chars = budget.per_entry_chars();
        let mut system_parts: Vec<String> = Vec::new();
        let mut user_question: Option<String> = None;
        for e in &self.head {
            match e {
                Entry::User(q) => user_question = Some(truncate_entry(q, per_chars)),
                other => system_parts.push(other.content(per_chars)),
            }
        }
        if let Some(ledger) = self.ledger.render() {
            system_parts.push(truncate_entry(&ledger, per_chars));
        }
        let mut out = vec![ChatMessage {
            role: ChatRole::System,
            content: system_parts.join("\n"),
        }];
        if let Some(q) = user_question {
            out.push(ChatMessage {
                role: ChatRole::User,
                content: q,
            });
        }

        let mut used = out.iter().map(|m| estimate_tokens(&m.content)).sum::<u32>();
        let total = budget.total_tokens.get();
        let mut tail_msgs: Vec<ChatMessage> = Vec::new();
        for e in self.tail.iter().rev() {
            let msg = ChatMessage {
                role: e.role(),
                content: e.content(per_chars),
            };
            let t = estimate_tokens(&msg.content);
            if used.saturating_add(t) > total {
                break;
            }
            used = used.saturating_add(t);
            tail_msgs.push(msg);
        }
        tail_msgs.reverse();
        out.extend(tail_msgs);
        out
    }
}

fn tokens_to_chars(tokens: u32) -> usize {
    (tokens as usize).saturating_mul(4)
}

pub fn estimate_tokens(text: &str) -> u32 {
    let n = text.chars().count() / 4;
    (n as u32).saturating_add(1)
}

fn truncate_entry(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut t = truncate_chars(s, max_chars.saturating_sub(14));
    t.push_str(" …(truncated)");
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tight_budget() -> PromptBudget {
        PromptBudget {
            total_tokens: NonZeroU32::new(200).unwrap(),
            per_entry_tokens: NonZeroU32::new(80).unwrap(),
        }
    }

    #[test]
    fn render_uses_system_and_user_roles() {
        let t = Transcript::start(
            "sys-prompt",
            "card",
            "contract-line",
            vec![],
            "the question",
        );
        let msgs = t.render(PromptBudget::from_model(8_192, 4_096));
        assert_eq!(msgs[0].role, ChatRole::System);
        assert!(msgs[0].content.contains("sys-prompt"));
        assert!(msgs[0].content.contains("card"));
        assert!(msgs[0].content.contains("contract-line"));
        assert_eq!(msgs[1].role, ChatRole::User);
        assert_eq!(msgs[1].content, "the question");
        assert!(msgs.iter().all(|m| m.role != ChatRole::Assistant));
    }

    #[test]
    fn newest_observation_is_kept_when_over_budget() {
        let mut t = Transcript::start("sys", "tools", "contract", vec![], "question");
        for i in 0..12 {
            t.push(Entry::Observation {
                tool: "t".into(),
                outcome: ToolOutcome::Ok(json!(format!("old-{i}-{}", "x".repeat(80)))),
            });
        }
        t.push(Entry::Observation {
            tool: "t".into(),
            outcome: ToolOutcome::Ok(json!(format!("NEWEST {}", "y".repeat(80)))),
        });
        let prompt: String = t
            .render(tight_budget())
            .into_iter()
            .map(|m| m.content)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            prompt.contains("NEWEST"),
            "newest observation dropped: {prompt}"
        );
    }

    #[test]
    fn heavy_prelude_keeps_user_question() {
        let prelude: Vec<Entry> = (0..40)
            .map(|i| Entry::Prelude(format!("Prelude line {i}")))
            .collect();
        let t = Transcript::start("sys", "tools", "contract", prelude, "keep-this-question");
        let msgs = t.render(PromptBudget::from_model(8_192, 4_096));
        let user = msgs
            .iter()
            .find(|m| m.role == ChatRole::User)
            .expect("user message");
        assert!(
            user.content.contains("keep-this-question"),
            "user question truncated out: {:?}",
            msgs
        );
    }

    #[test]
    fn ledger_is_in_system_head() {
        let mut t = Transcript::start("sys", "tools", "contract", vec![], "q");
        t.record_error("parse", "not json");
        let msgs = t.render(PromptBudget::from_model(8_192, 4_096));
        assert!(msgs[0].content.contains("Known failures"));
        assert!(msgs[0].content.contains("parse"));
    }

    #[test]
    fn prompt_budget_from_model_reserves_output() {
        let b = PromptBudget::from_model(8_192, 4_096);
        assert!(b.total_tokens.get() <= 8_192 - 4_096 + 512);
        assert!(b.per_entry_tokens.get() >= 256);
    }

    #[test]
    fn render_does_not_pad_empty_user() {
        let t = Transcript {
            head: vec![Entry::System("sys".into())],
            tail: vec![],
            ledger: ErrorLedger::default(),
        };
        let msgs = t.render(PromptBudget::from_model(8_192, 4_096));
        assert_eq!(msgs[0].role, ChatRole::System);
        assert!(
            msgs.iter().all(|m| m.role != ChatRole::User),
            "empty user pad: {msgs:?}"
        );
    }

    #[test]
    fn prompt_budget_from_resolved_matches_from_model() {
        let llm = crate::resolved_config::LlmResolved {
            context_length: Some(16_384),
            max_tokens: Some(2_048),
            ..Default::default()
        };
        assert_eq!(
            PromptBudget::from_resolved(&llm),
            PromptBudget::from_model(16_384, 2_048)
        );
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn render_never_exceeds_token_ceiling(n in 0usize..40, pad in 8usize..80) {
            let mut t = Transcript::start("sys", "tools", "contract", vec![], "question");
            for i in 0..n {
                t.push(Entry::Assistant(format!("asst-{i}-{}", "a".repeat(pad))));
                t.push(Entry::Observation {
                    tool: "t".into(),
                    outcome: crate::tools::ToolOutcome::Ok(serde_json::json!(format!("obs-{i}"))),
                });
            }
            let budget = PromptBudget::from_model(2_048, 512);
            let msgs = t.render(budget);
            let used: u32 = msgs.iter().map(|m| estimate_tokens(&m.content)).sum();
            prop_assert!(used <= budget.total_tokens.get());
            prop_assert_eq!(msgs[1].role, ChatRole::User);
            prop_assert!(msgs[1].content.contains("question"));
        }
    }
}
