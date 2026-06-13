#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;
use crate::saga_store::SagaStoreError;

#[test]
fn test_saga_step_creation() {
    let step = SagaStep::new(
        1,
        "orders-service",
        "Order",
        "createOrder",
        serde_json::json!({"id": "123"}),
        "deleteOrder",
        serde_json::json!({"id": "123"}),
    );

    assert_eq!(step.number, 1);
    assert_eq!(step.subgraph, "orders-service");
    assert_eq!(step.typename, "Order");
}

#[test]
fn test_compensation_strategy_default() {
    assert_eq!(CompensationStrategy::default(), CompensationStrategy::Automatic);
}

#[tokio::test]
async fn test_saga_coordinator_creation() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    assert_eq!(coordinator.strategy(), CompensationStrategy::Automatic);
}

#[tokio::test]
async fn test_create_saga_with_empty_steps() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let result = coordinator.create_saga(vec![]).await;
    assert!(
        matches!(result, Err(crate::saga_store::SagaStoreError::Database(_))),
        "expected Database error for empty steps, got: {result:?}"
    );
}

/// M-saga-coordinator: `create_saga` validates input but must fail loud rather
/// than hand back a UUID for a saga it never persisted.
#[tokio::test]
async fn test_create_saga_with_valid_steps_fails_loud() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let steps = vec![SagaStep::new(
        1,
        "service-1",
        "Type1",
        "mut1",
        serde_json::json!({}),
        "comp1",
        serde_json::json!({}),
    )];

    let result = coordinator.create_saga(steps).await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "create_saga must not fabricate an id for an unpersisted saga, got: {result:?}"
    );
}

/// The compensation strategy passed to `new` is retained.
#[test]
fn test_coordinator_retains_strategy() {
    assert_eq!(
        SagaCoordinator::new(CompensationStrategy::Manual).strategy(),
        CompensationStrategy::Manual
    );
}

/// M-saga-coordinator: `execute_saga` must fail loud — it previously returned a
/// fabricated `Completed` result for any id without touching the store.
#[tokio::test]
async fn execute_saga_fails_loud() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let result = coordinator.execute_saga(Uuid::new_v4()).await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "execute_saga must fail loud, got: {result:?}"
    );
}

/// M-saga-coordinator: the `operation` string must identify the failing entry point.
#[tokio::test]
async fn execute_saga_operation_is_descriptive() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let result = coordinator.execute_saga(Uuid::new_v4()).await;

    match result {
        Err(SagaStoreError::NotImplemented { operation }) => {
            assert_eq!(operation, "SagaCoordinator::execute_saga");
        },
        other => panic!("expected NotImplemented, got: {other:?}"),
    }
}

/// M-saga-coordinator: `get_saga_status` must fail loud — it previously
/// fabricated a `Pending` status for any id without consulting the store.
#[tokio::test]
async fn get_saga_status_fails_loud() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);

    let saga_id = Uuid::new_v4();
    let result = coordinator.get_saga_status(saga_id).await;

    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "get_saga_status must fail loud, got: {result:?}"
    );
}

/// M-saga-coordinator: `cancel_saga` must fail loud — it previously returned a
/// fabricated `Failed`/"cancelled" result without stopping or compensating anything.
#[tokio::test]
async fn cancel_saga_fails_loud() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let result = coordinator.cancel_saga(Uuid::new_v4()).await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "cancel_saga must fail loud, got: {result:?}"
    );
}

/// M-saga-coordinator: `get_saga_result` must fail loud — it previously returned
/// a fabricated `Completed` result for any id without loading from the store.
#[tokio::test]
async fn get_saga_result_fails_loud() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let result = coordinator.get_saga_result(Uuid::new_v4()).await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "get_saga_result must fail loud, got: {result:?}"
    );
}

/// M-saga-coordinator: `list_in_flight_sagas` must fail loud — it previously
/// returned an empty list without ever querying the store.
#[tokio::test]
async fn list_in_flight_sagas_fails_loud() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let result = coordinator.list_in_flight_sagas().await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "list_in_flight_sagas must fail loud, got: {result:?}"
    );
}

// Compile-time assertion: SagaCoordinator must be Send.
// This test will fail to compile if the Send bound is lost.
#[test]
fn saga_coordinator_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<SagaCoordinator>();
}
