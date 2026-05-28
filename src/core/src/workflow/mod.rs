pub mod outcome;
pub mod runner;

pub use outcome::PhaseOutcome;
pub use runner::{Config as WorkflowConfig, PhaseExecutor};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::suite::{WorkflowNodeContract, WorkflowSuiteContract};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionIntent {
    Annotation,
    Forward,
    Loopback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhaseDirective<P, R, G> {
    Transition {
        to: P,
        intent: TransitionIntent,
        reason_code: Option<R>,
        reason_detail: Option<Value>,
    },
    Block {
        phase: P,
        kind: G,
        reason: String,
    },
}

pub fn reason_detail_value(detail: &impl Serialize) -> Value {
    serde_json::to_value(detail).unwrap_or(Value::Null)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreTurnDirective<G = String> {
    Proceed,
    FailFast { kind: G, reason: String },
}

pub fn next_replan_backtracks(
    current: usize,
    intent: TransitionIntent,
    is_backtrack: bool,
    cap: usize,
) -> usize {
    match intent {
        TransitionIntent::Annotation => current,
        TransitionIntent::Forward => 0,
        TransitionIntent::Loopback => {
            if is_backtrack {
                current.saturating_add(1).min(cap)
            } else {
                current
            }
        }
    }
}

/// Executes suite-defined pre-turn guard logic through the typed workflow contract.
pub fn evaluate_pre_turn<C: WorkflowSuiteContract>(
    state: &C::State,
) -> PreTurnDirective<C::GuardKind> {
    C::pre_turn(state)
}

/// Applies a typed workflow event through the suite reducer contract.
pub fn reduce_event<C: WorkflowSuiteContract>(state: &mut C::State, event: C::Event) {
    C::reduce(state, event);
}

/// Derives the canonical phase from the suite's typed workflow node.
pub fn phase_from_state<C: WorkflowNodeContract>(state: &C::State) -> C::Phase {
    let node = C::node_from_state(state);
    C::phase_from_node(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suite::{WorkflowNodeContract, WorkflowSuiteContract};

    #[test]
    fn loopback_counter_rules() {
        assert_eq!(
            next_replan_backtracks(2, TransitionIntent::Annotation, true, 5),
            2
        );
        assert_eq!(
            next_replan_backtracks(2, TransitionIntent::Forward, true, 5),
            0
        );
        assert_eq!(
            next_replan_backtracks(2, TransitionIntent::Loopback, false, 5),
            2
        );
        assert_eq!(
            next_replan_backtracks(2, TransitionIntent::Loopback, true, 3),
            3
        );
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum DummyPhase {
        A,
        B,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    #[allow(dead_code)]
    enum DummyReason {
        X,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum DummyGuard {
        Blocked,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum DummyNode {
        A,
        B,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct DummyState {
        node: DummyNode,
        blocked: bool,
        count: usize,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum DummyEvent {
        Advance,
    }

    struct DummyContract;

    impl WorkflowSuiteContract for DummyContract {
        type Phase = DummyPhase;
        type ReasonCode = DummyReason;
        type GuardKind = DummyGuard;
        type State = DummyState;
        type Event = DummyEvent;

        fn phase_as_str(_phase: Self::Phase) -> &'static str {
            "dummy"
        }
        fn reason_as_str(_reason: Self::ReasonCode) -> &'static str {
            "dummy_reason"
        }
        fn guard_kind_as_str(_kind: Self::GuardKind) -> &'static str {
            "blocked"
        }
        fn is_backtrack(_from: Self::Phase, _to: Self::Phase) -> bool {
            false
        }
        fn replan_backtrack_cap() -> usize {
            1
        }
        fn pre_turn(state: &Self::State) -> PreTurnDirective<Self::GuardKind> {
            if state.blocked {
                PreTurnDirective::FailFast {
                    kind: DummyGuard::Blocked,
                    reason: "blocked".to_string(),
                }
            } else {
                PreTurnDirective::Proceed
            }
        }
        fn reduce(state: &mut Self::State, event: Self::Event) {
            match event {
                DummyEvent::Advance => {
                    state.node = DummyNode::B;
                    state.count += 1;
                }
            }
        }
    }

    impl WorkflowNodeContract for DummyContract {
        type Node = DummyNode;

        fn node_from_state(state: &Self::State) -> Self::Node {
            state.node
        }
        fn phase_from_node(node: Self::Node) -> Self::Phase {
            match node {
                DummyNode::A => DummyPhase::A,
                DummyNode::B => DummyPhase::B,
            }
        }
    }

    #[test]
    fn contract_helpers_route_through_typed_contract() {
        let mut st = DummyState {
            node: DummyNode::A,
            blocked: false,
            count: 0,
        };
        assert_eq!(phase_from_state::<DummyContract>(&st), DummyPhase::A);
        assert!(matches!(
            evaluate_pre_turn::<DummyContract>(&st),
            PreTurnDirective::Proceed
        ));
        reduce_event::<DummyContract>(&mut st, DummyEvent::Advance);
        assert_eq!(phase_from_state::<DummyContract>(&st), DummyPhase::B);
        assert_eq!(st.count, 1);
    }
}
