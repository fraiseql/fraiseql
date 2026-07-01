//! Unit tests for the pure coordinator decision helpers.
//!
//! These run in the fast `test` leg on every push (no database, feature-agnostic),
//! covering the tricky bits — verb→kind mapping, terminal-state classification, and
//! the divide-by-zero-safe percentage — that the wired coordinator relies on.

use super::{mutation_type_for, progress_percentage, saga_state_is_terminal};
use crate::saga_store::{MutationType, SagaState};

#[test]
fn mutation_type_for_maps_create_verbs() {
    assert_eq!(mutation_type_for("createOrder"), Some(MutationType::Create));
    assert_eq!(mutation_type_for("addItem"), Some(MutationType::Create));
    // Case-insensitive, mirroring the runtime resolver.
    assert_eq!(mutation_type_for("CreateOrder"), Some(MutationType::Create));
}

#[test]
fn mutation_type_for_maps_update_verbs() {
    assert_eq!(mutation_type_for("updateOrder"), Some(MutationType::Update));
    assert_eq!(mutation_type_for("modifyOrder"), Some(MutationType::Update));
}

#[test]
fn mutation_type_for_maps_delete_verbs() {
    assert_eq!(mutation_type_for("deleteOrder"), Some(MutationType::Delete));
    assert_eq!(mutation_type_for("removeOrder"), Some(MutationType::Delete));
}

#[test]
fn mutation_type_for_rejects_unknown_verb() {
    // An unrecognised name must not default to a kind (M-fed-mut-executor): the
    // coordinator refuses to persist a step whose mutation kind is unknowable.
    assert_eq!(mutation_type_for("frobnicateOrder"), None);
    assert_eq!(mutation_type_for(""), None);
}

#[test]
fn terminal_states_are_terminal() {
    assert!(saga_state_is_terminal(&SagaState::Completed));
    assert!(saga_state_is_terminal(&SagaState::Failed));
    assert!(saga_state_is_terminal(&SagaState::Compensated));
    assert!(saga_state_is_terminal(&SagaState::Cancelled));
}

#[test]
fn in_flight_states_are_not_terminal() {
    assert!(!saga_state_is_terminal(&SagaState::Pending));
    assert!(!saga_state_is_terminal(&SagaState::Executing));
    assert!(!saga_state_is_terminal(&SagaState::Compensating));
}

#[test]
fn progress_percentage_is_zero_for_no_steps() {
    // No steps must not divide by zero.
    assert!((progress_percentage(0, 0) - 0.0).abs() < f64::EPSILON);
}

#[test]
fn progress_percentage_full_is_exactly_100() {
    assert!((progress_percentage(2, 2) - 100.0).abs() < f64::EPSILON);
    assert!((progress_percentage(3, 3) - 100.0).abs() < f64::EPSILON);
}

#[test]
fn progress_percentage_partial() {
    assert!((progress_percentage(1, 4) - 25.0).abs() < f64::EPSILON);
    assert!((progress_percentage(0, 3) - 0.0).abs() < f64::EPSILON);
}
