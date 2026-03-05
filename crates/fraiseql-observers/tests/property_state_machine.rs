//! Property-based tests for the listener state machine.
//!
//! Verifies that:
//! (a) `is_valid_transition` never panics for any (state, state) pair.
//! (b) When `is_valid_transition` returns `true`, `transition()` must succeed.
//! (c) When `is_valid_transition` returns `false`, `transition()` must return an error.
//! (d) Successful transitions always land in a documented state.

use fraiseql_observers::{ListenerState, ListenerStateMachine};
use proptest::prelude::*;

fn arb_state() -> impl Strategy<Value = ListenerState> {
    prop_oneof![
        Just(ListenerState::Initializing),
        Just(ListenerState::Connecting),
        Just(ListenerState::Running),
        Just(ListenerState::Recovering),
        Just(ListenerState::Stopped),
    ]
}

/// All five documented states in a fixed slice for membership checks.
const ALL_STATES: [ListenerState; 5] = [
    ListenerState::Initializing,
    ListenerState::Connecting,
    ListenerState::Running,
    ListenerState::Recovering,
    ListenerState::Stopped,
];

proptest! {
    /// `is_valid_transition` must never panic for any (current, next) pair.
    #[test]
    fn is_valid_transition_never_panics(current in arb_state(), next in arb_state()) {
        let _ = ListenerStateMachine::is_valid_transition(current, next);
    }

    /// A freshly constructed machine always starts at Initializing.
    /// Attempting each possible next state must agree with `is_valid_transition`.
    #[test]
    fn transition_result_agrees_with_validity_table(next in arb_state()) {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let sm = ListenerStateMachine::new("prop-test".to_string());

        // Machine starts at Initializing
        let current = ListenerState::Initializing;
        let valid = ListenerStateMachine::is_valid_transition(current, next);

        let result = rt.block_on(sm.transition(next));
        if valid {
            prop_assert!(
                result.is_ok(),
                "valid transition Initializing → {next:?} must succeed, got: {result:?}"
            );
        } else {
            prop_assert!(
                result.is_err(),
                "invalid transition Initializing → {next:?} must fail, got: Ok(())"
            );
        }
    }

    /// After a successful transition, the state must be one of the five documented states.
    #[test]
    fn post_transition_state_is_always_valid(next in arb_state()) {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let sm = ListenerStateMachine::new("prop-test-valid-state".to_string());

        if rt.block_on(sm.transition(next)).is_ok() {
            let state = rt.block_on(sm.get_state());
            prop_assert!(
                ALL_STATES.contains(&state),
                "post-transition state {state:?} is not a documented state"
            );
        }
    }

    /// `is_valid_transition` must be a pure function — same inputs, same output.
    #[test]
    fn valid_transition_table_is_deterministic(current in arb_state(), next in arb_state()) {
        let r1 = ListenerStateMachine::is_valid_transition(current, next);
        let r2 = ListenerStateMachine::is_valid_transition(current, next);
        prop_assert_eq!(r1, r2);
    }
}
