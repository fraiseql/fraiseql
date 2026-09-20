//! Pure coordinator decision helpers.
//!
//! These functions hold the saga *coordination* logic with no I/O: deriving the
//! persisted [`MutationType`] a forward mutation name implies, deciding whether a
//! saga state is terminal (so a cancel is refused), and computing progress as a
//! percentage. Keeping them pure lets their unit tests run in the fast `test` leg
//! on every push, independently of a live database — the same arrangement the
//! forward and compensation phases use in [`crate::saga_executor::forward`] and
//! [`crate::saga_compensator::compensation`].

use crate::saga_store::{MutationType, SagaState};

/// Map a forward mutation operation name to the [`MutationType`] to persist for a
/// saga step.
///
/// Mirrors the runtime `determine_mutation_type` verb-prefix rule so a saga step
/// created from a coordinator `mutation_name` (e.g. `createOrder`) stores the same
/// kind the executor will dispatch. Returns `None` when the name begins with no
/// recognised verb — the coordinator then refuses to persist a step whose mutation
/// kind is unknowable rather than defaulting it (the M-fed-mut-executor trap, where
/// an unrecognised name silently became an `UPDATE`).
pub(super) fn mutation_type_for(mutation_name: &str) -> Option<MutationType> {
    let lower = mutation_name.to_lowercase();
    if lower.starts_with("create") || lower.starts_with("add") {
        Some(MutationType::Create)
    } else if lower.starts_with("update") || lower.starts_with("modify") {
        Some(MutationType::Update)
    } else if lower.starts_with("delete") || lower.starts_with("remove") {
        Some(MutationType::Delete)
    } else {
        None
    }
}

/// Whether `state` is a terminal saga state that can no longer be cancelled.
///
/// A saga that has already `Completed`, `Failed`, `Compensated`, or been
/// `Cancelled` has reached an outcome; a second cancel must fail loud rather than
/// re-drive it. `Pending`/`Executing`/`Compensating` are in-flight and cancellable.
pub(super) const fn saga_state_is_terminal(state: &SagaState) -> bool {
    match state {
        SagaState::Completed
        | SagaState::Failed
        | SagaState::Compensated
        | SagaState::Cancelled => true,
        SagaState::Pending | SagaState::Executing | SagaState::Compensating => false,
    }
}

/// Progress of a saga as a percentage of completed steps.
///
/// A saga with no steps is reported `0.0` rather than dividing by zero. Uses exact
/// `u32`→`f64` widening, so a fully-completed saga reports exactly `100.0`.
pub(super) fn progress_percentage(completed: u32, total: u32) -> f64 {
    if total == 0 {
        return 0.0;
    }
    f64::from(completed) / f64::from(total) * 100.0
}

#[cfg(test)]
mod tests;
