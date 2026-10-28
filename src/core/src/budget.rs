//! Run-level budget ceilings. Fixed in code; not env knobs.
//!
//! Two layers (do not collapse, do not duplicate):
//! - [`BudgetMeter`] on [`crate::agent::AgentCtx`] charges LLM/tool/agent-step work.
//! - [`crate::workflow::WorkflowError`] terminates the phase loop (hard ceiling / stuck / failed).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;

/// Hard ceiling on headless/auto-approve suite re-entries for one WS/headless run.
pub const MAX_AUTO_RESUMES: u8 = 8;

/// Conflict retries for a single audit append before the run stops.
pub const AUDIT_MAX_CONFLICT_RETRIES: u8 = 5;

/// Identical (tool, args) failures before the agent loop stops.
pub const MAX_REPEATED_ACTION_FAILURES: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetExhausted {
    AutoResumes,
    LlmCalls,
    ToolCalls,
    AgentSteps,
}

impl std::fmt::Display for BudgetExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AutoResumes => f.write_str("auto-resumes exhausted"),
            Self::LlmCalls => f.write_str("llm-call budget exhausted"),
            Self::ToolCalls => f.write_str("tool-call budget exhausted"),
            Self::AgentSteps => f.write_str("agent-step budget exhausted"),
        }
    }
}

impl std::error::Error for BudgetExhausted {}

/// Charge one auto-resume. `used` is the count already consumed this run.
pub fn charge_auto_resume(used: u8) -> Result<u8, BudgetExhausted> {
    let next = used.checked_add(1).ok_or(BudgetExhausted::AutoResumes)?;
    if next > MAX_AUTO_RESUMES {
        Err(BudgetExhausted::AutoResumes)
    } else {
        Ok(next)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charge {
    LlmCall,
    ToolCall,
    AgentStep,
}

/// Fixed ceilings for one agent run. Construct explicitly; not env knobs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunBudget {
    pub max_llm_calls: u32,
    pub max_tool_calls: u32,
    pub max_agent_steps: u32,
}

impl RunBudget {
    /// Default host budget (10 agent steps, 8 LLM/tool attempts per step).
    pub const STANDARD: Self = Self::agent_steps(10);

    pub const fn agent_steps(max_agent_steps: u32) -> Self {
        let per = if max_agent_steps == 0 {
            8
        } else if max_agent_steps > u32::MAX / 8 {
            u32::MAX
        } else {
            max_agent_steps * 8
        };
        let per = if per < 8 { 8 } else { per };
        Self {
            max_llm_calls: per,
            max_tool_calls: per,
            max_agent_steps,
        }
    }

    pub const fn with_agent_steps(self, max_agent_steps: u32) -> Self {
        Self::agent_steps(max_agent_steps)
    }
}

/// Shared run meter. `AgentCtx` clone shares the same counters.
#[derive(Debug)]
pub struct BudgetMeter {
    budget: std::sync::RwLock<RunBudget>,
    llm_calls: AtomicU32,
    tool_calls: AtomicU32,
    agent_steps: AtomicU32,
}

impl BudgetMeter {
    pub fn new(budget: RunBudget) -> Arc<Self> {
        Arc::new(Self {
            budget: std::sync::RwLock::new(budget),
            llm_calls: AtomicU32::new(0),
            tool_calls: AtomicU32::new(0),
            agent_steps: AtomicU32::new(0),
        })
    }

    pub fn snapshot(&self) -> RunBudget {
        *self.budget.read().expect("budget lock")
    }

    pub fn max_agent_steps(&self) -> u32 {
        self.snapshot().max_agent_steps
    }

    /// Discovery may raise the agent-step ceiling before the first charge.
    pub fn raise_agent_steps(&self, max_agent_steps: u32) {
        let mut g = self.budget.write().expect("budget lock");
        *g = g.with_agent_steps(max_agent_steps.max(g.max_agent_steps));
    }

    pub fn charge(&self, charge: Charge) -> Result<(), BudgetExhausted> {
        let b = self.snapshot();
        let (cell, max, err) = match charge {
            Charge::LlmCall => (&self.llm_calls, b.max_llm_calls, BudgetExhausted::LlmCalls),
            Charge::ToolCall => (
                &self.tool_calls,
                b.max_tool_calls,
                BudgetExhausted::ToolCalls,
            ),
            Charge::AgentStep => (
                &self.agent_steps,
                b.max_agent_steps,
                BudgetExhausted::AgentSteps,
            ),
        };
        loop {
            let n = cell.load(Ordering::SeqCst);
            if n >= max {
                return Err(err);
            }
            if cell
                .compare_exchange(n, n + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    pub fn llm_calls(&self) -> u32 {
        self.llm_calls.load(Ordering::SeqCst)
    }

    pub fn tool_calls(&self) -> u32 {
        self.tool_calls.load(Ordering::SeqCst)
    }

    pub fn agent_steps(&self) -> u32 {
        self.agent_steps.load(Ordering::SeqCst)
    }
}

/// Cooperative cancel flag for kernel awaits. Hosts clone and call [`Cancel::cancel`].
#[derive(Clone, Debug)]
pub struct Cancel {
    flag: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl Default for Cancel {
    fn default() -> Self {
        Self::new()
    }
}

impl Cancel {
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub async fn cancelled(&self) {
        loop {
            if self.is_cancelled() {
                return;
            }
            self.notify.notified().await;
        }
    }
}

/// Bounded repeated-(tool, args) failure detector for the agent loop.
#[derive(Debug, Default)]
pub struct ActionLedger {
    seen: std::collections::HashMap<(String, u64), u8>,
}

impl ActionLedger {
    pub fn record_failure(&mut self, tool: &str, args: &serde_json::Value) -> Result<u8, u8> {
        let fp = (tool.to_string(), hash_args(args));
        let n = self.seen.entry(fp).or_insert(0);
        *n = n.saturating_add(1);
        if *n >= MAX_REPEATED_ACTION_FAILURES {
            Err(*n)
        } else {
            Ok(*n)
        }
    }
}

fn hash_args(args: &serde_json::Value) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    args.to_string().hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn auto_resume_terminates_at_ceiling() {
        let mut used = 0u8;
        for _ in 0..MAX_AUTO_RESUMES {
            used = charge_auto_resume(used).expect("within ceiling");
        }
        assert_eq!(used, MAX_AUTO_RESUMES);
        assert_eq!(charge_auto_resume(used), Err(BudgetExhausted::AutoResumes));
        assert_eq!(
            charge_auto_resume(u8::MAX),
            Err(BudgetExhausted::AutoResumes)
        );
    }

    #[test]
    fn meter_rejects_llm_calls_past_ceiling() {
        let meter = BudgetMeter::new(RunBudget {
            max_llm_calls: 2,
            max_tool_calls: 8,
            max_agent_steps: 8,
        });
        meter.charge(Charge::LlmCall).expect("1");
        meter.charge(Charge::LlmCall).expect("2");
        assert_eq!(
            meter.charge(Charge::LlmCall),
            Err(BudgetExhausted::LlmCalls)
        );
        assert_eq!(meter.llm_calls(), 2);
    }

    #[test]
    fn cloned_meters_share_counters() {
        let meter = BudgetMeter::new(RunBudget::agent_steps(1));
        let clone = Arc::clone(&meter);
        clone.charge(Charge::AgentStep).expect("step");
        assert_eq!(
            meter.charge(Charge::AgentStep),
            Err(BudgetExhausted::AgentSteps)
        );
    }

    #[test]
    fn action_ledger_stops_repeated_identical_failures() {
        let mut ledger = ActionLedger::default();
        let args = serde_json::json!({"k": 1});
        assert!(ledger.record_failure("t", &args).is_ok());
        assert!(ledger.record_failure("t", &args).is_ok());
        assert_eq!(ledger.record_failure("t", &args), Err(3));
        assert!(ledger
            .record_failure("t", &serde_json::json!({"k": 2}))
            .is_ok());
    }

    #[tokio::test]
    async fn cancel_wakes_waiters() {
        let c = Cancel::new();
        let waiter = c.clone();
        let h = tokio::spawn(async move {
            waiter.cancelled().await;
        });
        c.cancel();
        h.await.expect("join");
        assert!(c.is_cancelled());
    }

    proptest! {
        #[test]
        fn charge_never_exceeds_ceiling(max in 0u32..32) {
            let meter = BudgetMeter::new(RunBudget {
                max_llm_calls: max,
                max_tool_calls: 32,
                max_agent_steps: 32,
            });
            let mut ok = 0u32;
            loop {
                match meter.charge(Charge::LlmCall) {
                    Ok(()) => ok += 1,
                    Err(BudgetExhausted::LlmCalls) => break,
                    Err(other) => panic!("unexpected {other:?}"),
                }
            }
            prop_assert_eq!(ok, max);
            prop_assert_eq!(meter.llm_calls(), max);
        }
    }
}
