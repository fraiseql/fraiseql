#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;
use crate::saga_store::SagaStoreError;

#[test]
fn test_saga_compensator_creation() {
    let compensator = SagaCompensator::new();
    drop(compensator);
}

#[test]
fn test_saga_compensator_default() {
    let _compensator = SagaCompensator::default();
    // Default should work
}

#[test]
fn test_saga_compensator_with_store() {
    // Test that we can create a compensator; full store testing requires a database.
    let compensator = SagaCompensator::new();
    assert!(!compensator.has_store());
}

#[test]
fn test_saga_compensator_has_store_method() {
    // Verify has_store() correctly reports status
    let compensator = SagaCompensator::new();
    assert!(!compensator.has_store());
}

/// H33: `compensate_step` must fail loud — it previously simulated a successful
/// compensation and persisted a fabricated `{"deleted": true}` document.
#[tokio::test]
async fn compensate_step_fails_loud() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let result = compensator
        .compensate_step(saga_id, 1, "testCompensation", &serde_json::json!({}), "test-service")
        .await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "compensate_step must fail loud, got: {result:?}"
    );
}

/// H33: the compensation driver must fail loud rather than reporting a
/// fabricated `Compensated` status.
#[tokio::test]
async fn compensate_saga_fails_loud() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let result = compensator.compensate_saga(saga_id).await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "compensate_saga must fail loud, got: {result:?}"
    );
}

/// H33: every reverse-order compensation must fail loud (no fabricated success).
#[tokio::test]
async fn compensate_step_reverse_order_all_fail_loud() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    for step_num in (1..=3).rev() {
        let result = compensator
            .compensate_step(saga_id, step_num, "deleteEntity", &serde_json::json!({}), "svc")
            .await;
        assert!(
            matches!(result, Err(SagaStoreError::NotImplemented { .. })),
            "compensate_step {step_num} must fail loud, got: {result:?}"
        );
    }
}

/// H33: the `operation` string must identify the failing entry point.
#[tokio::test]
async fn compensate_step_operation_is_descriptive() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let result = compensator
        .compensate_step(saga_id, 1, "deleteEntity", &serde_json::json!({}), "svc")
        .await;

    match result {
        Err(SagaStoreError::NotImplemented { operation }) => {
            assert_eq!(operation, "SagaCompensator::compensate_step");
        },
        other => panic!("expected NotImplemented, got: {other:?}"),
    }
}

/// `get_compensation_status` is a read-only status query that never persists
/// state; without a store it honestly reports `None`.
#[tokio::test]
async fn get_compensation_status_without_store_is_none() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let status = compensator
        .get_compensation_status(saga_id)
        .await
        .unwrap_or_else(|e| panic!("expected Ok from get_compensation_status: {e}"));

    assert!(status.is_none(), "no store should yield no status");
}
