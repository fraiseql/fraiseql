//! Pure recovery-phase decision helpers.
//!
//! These functions hold the saga recovery *logic* with no I/O: deciding whether a
//! saga's persisted state warrants re-execution, and rendering the audit line
//! logged before a replay. Keeping them pure lets their unit tests run in the fast
//! `test` leg on every push — so the "only re-drive genuinely in-flight sagas"
//! contract is covered independently of a live database and of the `unstable-saga`
//! feature, the same arrangement the forward and compensation phases use in
//! [`crate::saga_executor::forward`] and [`crate::saga_compensator::compensation`].
//!
//! They are consumed only by the feature-gated wired recovery loop, so without
//! `unstable-saga` they are dead in a non-test build — hence the module-level
//! `allow(dead_code)` for that configuration (the established `#428` pattern).
#![cfg_attr(not(feature = "unstable-saga"), allow(dead_code))]

use uuid::Uuid;

use crate::saga_store::SagaState;

/// Whether a saga in `state` warrants re-execution by the recovery loop.
///
/// Only [`SagaState::Pending`] (never started) and [`SagaState::Executing`] (left
/// mid-flight by a crash) are safe to blindly re-drive forward. A saga that has
/// already reached a terminal state ([`SagaState::Completed`] /
/// [`SagaState::Failed`]) or entered the compensation lifecycle
/// ([`SagaState::Compensating`] / [`SagaState::Compensated`]) must be left alone —
/// replaying it would repeat or contradict work that already resolved (#429).
pub(super) const fn saga_is_recoverable(state: &SagaState) -> bool {
    matches!(state, SagaState::Pending | SagaState::Executing)
}

/// Render the audit line logged immediately before a saga is replayed.
///
/// Includes the saga id and the recovery attempt number so operators can trace how
/// many times a crash-interrupted saga has been re-driven.
pub(super) fn recovery_log_line(saga_id: Uuid, attempt: u32) -> String {
    format!("recovering saga {saga_id} (attempt {attempt})")
}

#[cfg(test)]
mod tests;
