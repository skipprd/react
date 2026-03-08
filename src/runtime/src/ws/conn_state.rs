use super::util::{truncate_title, DEFAULT_AGENT_TYPE};
use crate::run::event_hub::EventHub;

const SENT_BUFFER_CAPACITY: usize = 500;
use crate::ws::api_gen::src::models as api;
use crate::ws::terminal::{self, TerminalSink};
use react_core::session::{ControlStateStore, ThreadLog, ThreadLogReader, ThreadStep, ThreadStore};
use react_core::suite::{SuiteCtx, SuiteRegistry};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub(super) struct ConnState {
    pub seq: i32,
    pub sent: VecDeque<(i32, String)>,
    pub thread_seq: HashMap<String, i32>,
    pub seen: HashMap<String, i32>,
    pub current_suite: HashMap<String, String>,
    pub current_agent: HashMap<String, String>,
    pub reg: Arc<SuiteRegistry>,
    pub suite_ctx: SuiteCtx,
    pub terminal: Option<TerminalSink>,
    pub hub: Option<EventHub>,
}

impl ConnState {
    pub fn new(reg: Arc<SuiteRegistry>, suite_ctx: SuiteCtx, hub: Option<EventHub>) -> Self {
        Self {
            seq: 0,
            sent: VecDeque::new(),
            thread_seq: HashMap::new(),
            seen: HashMap::new(),
            current_suite: HashMap::new(),
            current_agent: HashMap::new(),
            reg,
            suite_ctx,
            terminal: terminal::sink().cloned(),
            hub,
        }
    }
    pub fn term(&self) -> Option<&TerminalSink> {
        self.terminal.as_ref()
    }
    pub fn hub(&self) -> Option<&EventHub> {
        self.hub.as_ref()
    }
    pub fn thread_store(&self) -> ThreadStore {
        ThreadStore::new(
            self.suite_ctx.storage().clone(),
            self.suite_ctx.scope().clone(),
            self.suite_ctx.keyspace().clone(),
        )
    }

    pub fn control_store(&self) -> ControlStateStore {
        ControlStateStore::new(
            self.suite_ctx.storage().clone(),
            self.suite_ctx.scope().clone(),
            self.suite_ctx.keyspace().clone(),
        )
    }

    /// Read-only ThreadLog access. Prefer this over thread_store() for read operations.
    pub fn log_reader(&self) -> impl ThreadLogReader {
        self.thread_store()
    }
    pub fn next_seq(&mut self) -> i32 {
        self.seq += 1;
        self.seq
    }
    pub fn next_thread_seq(&mut self, thread_id: &str) -> i32 {
        let entry = self.thread_seq.entry(thread_id.to_string()).or_insert(0);
        *entry += 1;
        *entry
    }
    pub fn buffer_last(&mut self, json: &str) {
        if let Ok(v) = serde_json::from_str::<Value>(json) {
            if let Some(seq) = v.get("seq").and_then(|x| x.as_i64()) {
                self.sent.push_back((seq as i32, json.to_string()));
                while self.sent.len() > SENT_BUFFER_CAPACITY {
                    self.sent.pop_front();
                }
            }
        }
    }
}

pub(super) fn derive_thread_context(log: &ThreadLog) -> (String, String) {
    let mut suite_id = String::new();
    let mut agent_type = DEFAULT_AGENT_TYPE.to_string();
    for step in log.steps.iter() {
        match step {
            ThreadStep::SwitchSuite { to, .. } => {
                if !to.trim().is_empty() {
                    suite_id = to.to_string();
                }
            }
            ThreadStep::SwitchAgent { to, .. } => {
                if !to.trim().is_empty() {
                    agent_type = to.to_string();
                }
            }
            _ => {}
        }
    }
    (suite_id, agent_type)
}

pub(super) fn default_suite_id(reg: &SuiteRegistry) -> Option<String> {
    reg.list_ids().into_iter().next().map(|s| s.to_string())
}

pub(super) async fn resolve_suite_id_for_thread(state: &mut ConnState, thread_id: &str) -> String {
    if let Some(s) = state.current_suite.get(thread_id).cloned() {
        return s;
    }
    let control = ControlStateStore::new(
        state.suite_ctx.storage().clone(),
        state.suite_ctx.scope().clone(),
        state.suite_ctx.keyspace().clone(),
    );
    if let Ok(Some(sid)) = control.load_suite_id(thread_id).await {
        if !sid.trim().is_empty() {
            state.current_suite.insert(thread_id.to_string(), sid.clone());
            return sid;
        }
    }
    if let Ok(log) = state.thread_store().get(thread_id).await {
        let (mut suite_id, _agent_type) = derive_thread_context(&log);
        if suite_id.trim().is_empty() {
            suite_id = default_suite_id(&state.reg).unwrap_or_default();
        }
        if !suite_id.trim().is_empty() {
            state
                .current_suite
                .insert(thread_id.to_string(), suite_id.clone());
            return suite_id;
        }
    }
    default_suite_id(&state.reg).unwrap_or_default()
}

pub(super) fn build_suites_catalog(reg: &SuiteRegistry) -> Vec<api::SuitesResponseSuitesInner> {
    let mut out: Vec<api::SuitesResponseSuitesInner> = Vec::new();
    for id in reg.list_ids() {
        if let Some(suite) = reg.get(id) {
            let mut s = api::SuitesResponseSuitesInner::new(
                id.to_string(),
                suite.supported_agent_types(),
            );
            s.label = Some(suite.label().to_string());
            s.default_agent_type = Some(suite.default_agent_type().to_string());
            out.push(s);
        }
    }
    out
}

pub(super) async fn synthesize_title(llm: &react_core::llm::DynLlm, question: &str, answer: &str) -> String {
    let prompt = format!(
		"Create a very short, descriptive chat title (≤ 8 words).\nRules: plain text only, no quotes, no punctuation beyond spaces, title case.\nQuestion: {}\nAnswer: {}\nTitle:",
		question, answer
	);
    let out = tokio::task::spawn_blocking({
        let llm2 = llm.clone();
        let p = prompt.clone();
        move || {
            llm2.chat(
                &[crate::llm::ChatMessage {
                    role: crate::llm::ChatRole::User,
                    content: p,
                }],
                &react_core::llm::LlmCallOptions {
                    prompt_id: "react.ws.synthesize_title",
                    thread_id: None,
                    expected_format: react_core::llm::LlmExpectedFormat::Text,
                    max_output_tokens: None,
                    temperature: None,
                    top_p: None,
                    reasoning_effort: None,
                    timeout_secs: None,
                },
            )
        }
    })
    .await;
    if let Ok(Ok(text)) = out {
        let t = text.trim();
        if !t.is_empty() {
            let norm = t.split_whitespace().collect::<Vec<_>>().join(" ");
            return truncate_title(&norm, 64);
        }
    }
    truncate_title(question, 64)
}

pub(super) fn normalize_agent_new(a: api::new_request::AgentType) -> String {
    match a {
        api::new_request::AgentType::Ask => "ask".to_string(),
        api::new_request::AgentType::Agent => "agent".to_string(),
        api::new_request::AgentType::Review => "review".to_string(),
        api::new_request::AgentType::Kb => "kb".to_string(),
    }
}

pub(super) fn normalize_agent_open(a: api::open_request::AgentType) -> String {
    match a {
        api::open_request::AgentType::Ask => "ask".to_string(),
        api::open_request::AgentType::Agent => "agent".to_string(),
        api::open_request::AgentType::Review => "review".to_string(),
        api::open_request::AgentType::Kb => "kb".to_string(),
    }
}

pub(super) async fn load_latest_plans(
    reg: &SuiteRegistry,
    suite_id: &str,
    ctx: &SuiteCtx,
    thread_id: &str,
) -> Vec<api::PlanSnapshot> {
    if let Some(suite) = reg.get(suite_id) {
        suite
            .load_ws_plans(thread_id, ctx)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| serde_json::from_value::<api::PlanSnapshot>(v).ok())
            .collect()
    } else {
        Vec::new()
    }
}
