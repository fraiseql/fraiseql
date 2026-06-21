//! Pure forward-phase decision helpers.
//!
//! These functions hold the saga forward-phase *logic* with no I/O: mapping a
//! mutation outcome to a [`StepExecutionResult`] + persisted [`StepState`], and
//! deriving the terminal [`SagaState`] from the per-step results. Keeping them
//! pure lets their unit tests run in the fast `test` leg on every push, so the
//! "never fabricate success" contract is covered independently of a live
//! database and of the `unstable-saga` feature.
//!
//! They are consumed only by the feature-gated wired executor, so without
//! `unstable-saga` they are dead in a non-test build — hence the module-level
//! `allow(dead_code)` for that configuration (the established `#428` pattern).
#![cfg_attr(not(feature = "unstable-saga"), allow(dead_code))]

use fraiseql_error::Result;
use serde_json::Value;

use super::StepExecutionResult;
use crate::saga_store::{SagaState, StepState};

/// Map a single step's mutation `outcome` to the result reported to callers and
/// the [`StepState`] to persist.
///
/// A mutation `Err` is a *legitimate saga outcome*, not an infrastructure
/// failure: it becomes `success: false` with the error message captured, and the
/// step is persisted [`StepState::Failed`]. Forward execution never fabricates a
/// `Completed` step from a failed mutation (audit H32).
pub(super) fn step_result_from(
    step_number: u32,
    outcome: &Result<Value>,
    duration_ms: u64,
) -> (StepExecutionResult, StepState) {
    match outcome {
        Ok(data) => (
            StepExecutionResult {
                step_number,
                success: true,
                data: Some(data.clone()),
                error: None,
                duration_ms,
            },
            StepState::Completed,
        ),
        Err(error) => (
            StepExecutionResult {
                step_number,
                success: false,
                data: None,
                error: Some(error.to_string()),
                duration_ms,
            },
            StepState::Failed,
        ),
    }
}

/// Derive the saga's terminal state from the forward-phase step results: every
/// step succeeded ⇒ [`SagaState::Completed`]; any step failed ⇒
/// [`SagaState::Failed`] (compensation is decided separately). An empty result
/// set is vacuously `Completed`.
pub(super) fn saga_state_for(results: &[StepExecutionResult]) -> SagaState {
    if results.iter().all(|result| result.success) {
        SagaState::Completed
    } else {
        SagaState::Failed
    }
}

#[cfg(test)]
mod tests;
