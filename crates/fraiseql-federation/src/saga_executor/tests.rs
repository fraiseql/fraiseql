#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use uuid::Uuid;

use super::SagaExecutor;
use crate::saga_store::SagaStoreError;

#[test]
fn test_saga_executor_creation() {
    let executor = SagaExecutor::new();
    drop(executor);
}

#[test]
fn test_saga_executor_default() {
    let _executor = SagaExecutor::default();
    // Default should work
}

#[test]
fn test_saga_executor_with_store() {
    // Test that we can create an executor; full store testing requires a
    // database (integration tests).
    let executor = SagaExecutor::new();
    assert!(!executor.has_store());
}

#[test]
fn test_saga_executor_has_store_method() {
    let executor = SagaExecutor::new();
    assert!(!executor.has_store());
}

/// H32: `execute_step` must fail loud — distributed saga execution is unwired,
/// so it must never fabricate a result or persist a step transition.
#[tokio::test]
async fn execute_step_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor
        .execute_step(saga_id, 1, "testMutation", &serde_json::json!({}), "test-service")
        .await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_step must fail loud, got: {result:?}"
    );
}

/// H32: the no-store path must also fail loud (it previously returned a
/// fabricated placeholder success).
#[tokio::test]
async fn execute_step_without_store_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor
        .execute_step(saga_id, 1, "createOrder", &serde_json::json!({}), "orders-service")
        .await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_step without store must fail loud, got: {result:?}"
    );
}

/// H32: the forward-phase driver must fail loud rather than reporting empty or
/// fabricated step results.
#[tokio::test]
async fn execute_saga_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor.execute_saga(saga_id).await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_saga must fail loud, got: {result:?}"
    );
}

/// H32: `get_execution_state` derived its values from fabricated step states; it
/// must now fail loud.
#[tokio::test]
async fn get_execution_state_fails_loud() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor.get_execution_state(saga_id).await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "get_execution_state must fail loud, got: {result:?}"
    );
}

/// H32: the `operation` string must identify the failing entry point so callers
/// and logs can tell the unwired paths apart.
#[tokio::test]
async fn execute_step_operation_is_descriptive() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor
        .execute_step(saga_id, 1, "mutation", &serde_json::json!({}), "service")
        .await;

    match result {
        Err(SagaStoreError::NotImplemented { operation }) => {
            assert_eq!(operation, "SagaExecutor::execute_step");
        },
        other => panic!("expected NotImplemented, got: {other:?}"),
    }
}
