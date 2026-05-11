use super::*;

#[test]
fn test_valid_transitions() {
    let mut state = ConnectionState::Initial;
    state
        .transition(ConnectionState::AwaitingAuth)
        .unwrap_or_else(|e| panic!("expected Ok transitioning Initial->AwaitingAuth: {e}"));
    state
        .transition(ConnectionState::Authenticating)
        .unwrap_or_else(|e| {
            panic!("expected Ok transitioning AwaitingAuth->Authenticating: {e}")
        });
    state
        .transition(ConnectionState::Idle)
        .unwrap_or_else(|e| panic!("expected Ok transitioning Authenticating->Idle: {e}"));
}

#[test]
fn test_invalid_transition() {
    let mut state = ConnectionState::Initial;
    let result = state.transition(ConnectionState::Idle);
    assert!(
        matches!(result, Err(WireError::InvalidState { .. })),
        "expected InvalidState error for Initial->Idle, got: {result:?}"
    );
}

#[test]
fn test_close_from_any_state() {
    let mut state = ConnectionState::QueryInProgress;
    state
        .transition(ConnectionState::Closed)
        .unwrap_or_else(|e| panic!("expected Ok transitioning QueryInProgress->Closed: {e}"));
}
