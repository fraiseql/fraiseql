use uuid::Uuid;

use super::*;
use crate::saga_store::SagaState;

#[test]
fn pending_saga_is_recoverable() {
    assert!(
        saga_is_recoverable(&SagaState::Pending),
        "a never-started saga must be re-driven"
    );
}

#[test]
fn executing_saga_is_recoverable() {
    assert!(
        saga_is_recoverable(&SagaState::Executing),
        "a saga left Executing by a crash must be re-driven"
    );
}

#[test]
fn terminal_and_compensation_states_are_not_recoverable() {
    // Completed / Failed / Compensating / Compensated are all past the point
    // where a blind forward replay is safe.
    for state in [
        SagaState::Completed,
        SagaState::Failed,
        SagaState::Compensating,
        SagaState::Compensated,
    ] {
        assert!(
            !saga_is_recoverable(&state),
            "{state:?} must not be re-driven by the recovery loop",
        );
    }
}

#[test]
fn recovery_log_line_is_non_empty_and_names_the_saga() {
    let saga_id = Uuid::new_v4();
    let line = recovery_log_line(saga_id, 2);

    assert!(!line.is_empty(), "recovery log line must not be empty");
    assert!(
        line.contains(&saga_id.to_string()),
        "recovery log line must identify the saga: {line}"
    );
    assert!(line.contains('2'), "recovery log line must mention the attempt count: {line}");
}
