//! Pure compensation-phase decision helpers.
//!
//! These functions hold the saga compensation *logic* with no I/O: deciding
//! whether a step can be compensated, mapping a compensation mutation outcome to
//! a [`CompensationStepResult`], and ordering the completed steps for reverse
//! rollback. Keeping them pure lets their unit tests run in the fast `test` leg
//! on every push, so the "never fabricate a rollback" contract is covered
//! independently of a live database and of the `saga` feature — the same
//! arrangement the forward phase uses in [`crate::saga_executor::forward`].
//!
//! They are consumed only by the feature-gated wired compensator, so without
//! `saga` they are dead in a non-test build — hence the module-level
//! `allow(dead_code)` for that configuration (the established `#428` pattern).
#![cfg_attr(not(feature = "saga"), allow(dead_code))]

use fraiseql_error::Result;
use serde_json::Value;

use super::CompensationStepResult;
use crate::saga_store::{SagaStep, StepState};

/// Whether `step` has a registered compensation (inverse) mutation.
///
/// A step is compensatable only when its `compensation_mutation` is present and
/// non-empty; a `None`/empty value means no rollback was registered and the step
/// must be skipped rather than silently treated as compensated (best-effort
/// contract, #429).
pub(super) fn step_is_compensatable(step: &SagaStep) -> bool {
    step.compensation_mutation.as_deref().is_some_and(|m| !m.is_empty())
}

/// Map a single compensation mutation `outcome` to the [`CompensationStepResult`]
/// reported to callers.
///
/// A compensation mutation `Err` is a *legitimate outcome*, not fabricated
/// success: it becomes `success: false` with the error captured, and no result
/// data. Compensation never reports a rollback that did not happen (audit H33) —
/// the true analog of the forward phase's
/// [`step_result_from`](crate::saga_executor::forward).
pub(super) fn compensation_result_from(
    step_number: u32,
    outcome: &Result<Value>,
    duration_ms: u64,
) -> CompensationStepResult {
    match outcome {
        Ok(data) => CompensationStepResult {
            step_number,
            success: true,
            data: Some(data.clone()),
            error: None,
            duration_ms,
        },
        Err(error) => CompensationStepResult {
            step_number,
            success: false,
            data: None,
            error: Some(error.to_string()),
            duration_ms,
        },
    }
}

/// Order the steps that must be compensated: only [`StepState::Completed`] steps
/// were actually executed and therefore need rolling back, in strict reverse
/// execution order (highest `order` first).
///
/// Steps that never completed (Pending/Executing/Failed) are excluded — there is
/// nothing to undo. Compensatability (a registered compensation mutation) is
/// decided per step by the caller via [`step_is_compensatable`], so a completed
/// step with no compensation still appears here and is reported as a best-effort
/// miss rather than being silently dropped.
pub(super) fn compensation_order(steps: &[SagaStep]) -> Vec<&SagaStep> {
    let mut completed: Vec<&SagaStep> =
        steps.iter().filter(|step| step.state == StepState::Completed).collect();
    // Reverse execution order: undo the most recently completed step first.
    completed.sort_by(|a, b| b.order.cmp(&a.order));
    completed
}

#[cfg(test)]
mod tests;
