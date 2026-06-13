//! Integration tests for `SagaExecutor` and `SagaCompensator`.
//!
//! Distributed saga execution/compensation is unwired (audit H32/H33): the
//! previous bodies fabricated success and persisted it. Every execution entry
//! point now fails loud with `SagaStoreError::NotImplemented`. These tests pin
//! that contract from outside the crate. Real execution is tracked in
//! <https://github.com/fraiseql/fraiseql/issues/429>.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_federation::{SagaCompensator, SagaExecutor, saga_store::SagaStoreError};
use serde_json::json;
use uuid::Uuid;

// ── Forward phase fails loud ─────────────────────────────────────────────────

#[tokio::test]
async fn test_execute_step_fails_loud() {
    let executor = SagaExecutor::new();
    let result = executor
        .execute_step(Uuid::new_v4(), 1, "createOrder", &json!({"id": "o1"}), "orders")
        .await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_step must fail loud, never fabricate a Completed step: {result:?}"
    );
}

#[tokio::test]
async fn test_execute_saga_fails_loud() {
    let executor = SagaExecutor::new();
    let result = executor.execute_saga(Uuid::new_v4()).await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_saga must fail loud, never return fabricated results: {result:?}"
    );
}

#[tokio::test]
async fn test_get_execution_state_fails_loud() {
    let executor = SagaExecutor::new();
    let result = executor.get_execution_state(Uuid::new_v4()).await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "get_execution_state must fail loud: {result:?}"
    );
}

// ── Compensation phase fails loud ────────────────────────────────────────────

#[tokio::test]
async fn test_compensate_saga_fails_loud() {
    let compensator = SagaCompensator::new();
    let result = compensator.compensate_saga(Uuid::new_v4()).await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "compensate_saga must fail loud, never mark a saga Compensated having undone nothing: {result:?}"
    );
}

#[tokio::test]
async fn test_compensation_status_query_no_store_is_none() {
    // The status *query* is read-only and never persists: with no store and no
    // compensation ever written it honestly reports "unknown" (None).
    let compensator = SagaCompensator::new();
    let status = compensator.get_compensation_status(Uuid::new_v4()).await.unwrap();
    assert!(status.is_none(), "no-store compensation status must return None");
}
