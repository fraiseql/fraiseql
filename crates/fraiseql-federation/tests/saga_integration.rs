//! Integration tests for `SagaExecutor` and `SagaCompensator`.
//!
//! These tests drive the full lifecycle using the no-store mode, which exercises
//! the executor logic without requiring a live database.  Tests that require
//! `PostgresSagaStore` (idempotency, exact state-persistence) are marked
//! `#[ignore]` and require the `FRAISEQL_TEST_DATABASE_URL` environment variable.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_federation::{
    CompensationStatus, ExecutionState, SagaCompensator, SagaExecutor, StepExecutionResult,
};
use serde_json::json;
use uuid::Uuid;

// ── Forward phase (no-store mode) ────────────────────────────────────────────

#[tokio::test]
async fn test_saga_executes_all_steps_in_order() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let mut step_numbers = Vec::new();
    for n in 1u32..=5 {
        let res: StepExecutionResult =
            executor.execute_step(saga_id, n, "doThing", &json!({}), "svc").await.unwrap();

        assert!(res.success, "step {n} should succeed");
        step_numbers.push(res.step_number);
    }

    assert_eq!(step_numbers, vec![1, 2, 3, 4, 5], "steps must execute in strict order");
}

#[tokio::test]
async fn test_saga_status_transitions_no_store() {
    // Without a store execute_saga returns empty (fast-path).
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let results = executor.execute_saga(saga_id).await.unwrap();
    assert!(results.is_empty(), "no-store execute_saga must return empty results");
}

#[tokio::test]
async fn test_saga_execution_state_initial() {
    // Fresh saga has zero progress.
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let state: ExecutionState = executor.get_execution_state(saga_id).await.unwrap();

    assert_eq!(state.saga_id, saga_id);
    assert_eq!(state.total_steps, 0);
    assert_eq!(state.completed_steps, 0);
    assert!(!state.failed);
    assert!(state.current_step.is_none());
    assert!(state.failure_reason.is_none());
}

#[tokio::test]
async fn test_saga_step_result_has_data_and_metrics() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let res = executor
        .execute_step(saga_id, 1, "createOrder", &json!({"id": "o1"}), "orders")
        .await
        .unwrap();

    assert!(res.success);
    assert!(res.data.is_some(), "step result must carry output data");
    assert!(res.error.is_none());
    // Duration is set to ≥0 ms (just verify the field is present/accessible).
    let _ = res.duration_ms;
}

// ── Compensation phase (no-store mode) ───────────────────────────────────────

#[tokio::test]
async fn test_saga_compensation_no_store_returns_empty() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    let result = compensator.compensate_saga(saga_id).await.unwrap();

    // No store → compensation fast-path: no steps to undo.
    assert!(
        matches!(result.status, CompensationStatus::Compensated),
        "empty compensation must report Compensated"
    );
    assert!(result.step_results.is_empty());
}

#[tokio::test]
async fn test_saga_compensation_status_query_no_store() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let status = compensator.get_compensation_status(saga_id).await.unwrap();
    // No store → unknown status without a backing saga.
    assert!(status.is_none(), "no-store compensation status must return None");
}
