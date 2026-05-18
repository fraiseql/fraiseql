#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

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

#[tokio::test]
async fn test_create_saga_with_valid_steps() {
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
    result.unwrap_or_else(|e| panic!("create_saga with valid steps should succeed: {e}"));
}

#[tokio::test]
async fn test_coordinator_with_executor() {
    let coordinator =
        SagaCoordinator::new(CompensationStrategy::Automatic).with_executor(Arc::new(()));

    assert_eq!(coordinator.strategy(), CompensationStrategy::Automatic);
}

#[tokio::test]
async fn test_coordinator_with_compensator() {
    let coordinator =
        SagaCoordinator::new(CompensationStrategy::Automatic).with_compensator(Arc::new(()));

    assert_eq!(coordinator.strategy(), CompensationStrategy::Automatic);
}

#[tokio::test]
async fn test_coordinator_with_both_executor_and_compensator() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic)
        .with_executor(Arc::new(()))
        .with_compensator(Arc::new(()));

    assert_eq!(coordinator.strategy(), CompensationStrategy::Automatic);
}

#[tokio::test]
async fn test_coordinator_wiring_with_manual_strategy() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Manual)
        .with_executor(Arc::new(()))
        .with_compensator(Arc::new(()));

    assert_eq!(coordinator.strategy(), CompensationStrategy::Manual);
}

#[tokio::test]
async fn test_saga_coordinator_full_workflow_single_step() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic)
        .with_executor(Arc::new(()))
        .with_compensator(Arc::new(()));

    // Create saga
    let steps = vec![SagaStep::new(
        1,
        "service-1",
        "User",
        "createUser",
        serde_json::json!({"name": "Alice"}),
        "deleteUser",
        serde_json::json!({}),
    )];

    let id = coordinator
        .create_saga(steps)
        .await
        .unwrap_or_else(|e| panic!("create_saga should succeed: {e}"));

    // Execute saga
    coordinator
        .execute_saga(id)
        .await
        .unwrap_or_else(|e| panic!("execute_saga should succeed: {e}"));
}

#[tokio::test]
async fn test_saga_coordinator_full_workflow_multiple_steps() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic)
        .with_executor(Arc::new(()))
        .with_compensator(Arc::new(()));

    // Create saga with multiple steps
    let steps = vec![
        SagaStep::new(
            1,
            "service-1",
            "User",
            "createUser",
            serde_json::json!({"name": "Alice"}),
            "deleteUser",
            serde_json::json!({}),
        ),
        SagaStep::new(
            2,
            "service-2",
            "Account",
            "createAccount",
            serde_json::json!({"user_id": "alice"}),
            "deleteAccount",
            serde_json::json!({}),
        ),
        SagaStep::new(
            3,
            "service-3",
            "Subscription",
            "createSubscription",
            serde_json::json!({"user_id": "alice", "plan": "premium"}),
            "cancelSubscription",
            serde_json::json!({}),
        ),
    ];

    let id = coordinator
        .create_saga(steps)
        .await
        .unwrap_or_else(|e| panic!("create_saga with multiple steps should succeed: {e}"));

    // Execute saga
    coordinator
        .execute_saga(id)
        .await
        .unwrap_or_else(|e| panic!("execute_saga with multiple steps should succeed: {e}"));
}

#[tokio::test]
async fn test_saga_coordinator_get_status() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic)
        .with_executor(Arc::new(()))
        .with_compensator(Arc::new(()));

    let saga_id = Uuid::new_v4();
    let status = coordinator.get_saga_status(saga_id).await;

    let status = status.unwrap_or_else(|e| panic!("get_saga_status should succeed: {e}"));
    assert_eq!(status.saga_id, saga_id);
}

#[tokio::test]
async fn test_saga_coordinator_cancel_saga() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic)
        .with_executor(Arc::new(()))
        .with_compensator(Arc::new(()));

    let saga_id = Uuid::new_v4();
    let result = coordinator.cancel_saga(saga_id).await;

    let result = result.unwrap_or_else(|e| panic!("cancel_saga should succeed: {e}"));
    assert_eq!(result.saga_id, saga_id);
    assert_eq!(result.state, SagaState::Failed);
}

#[tokio::test]
async fn test_saga_coordinator_get_result() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic)
        .with_executor(Arc::new(()))
        .with_compensator(Arc::new(()));

    let saga_id = Uuid::new_v4();
    let result = coordinator.get_saga_result(saga_id).await;

    result.unwrap_or_else(|e| panic!("get_saga_result should succeed: {e}"));
}

#[tokio::test]
async fn test_saga_coordinator_list_in_flight() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic)
        .with_executor(Arc::new(()))
        .with_compensator(Arc::new(()));

    let result = coordinator.list_in_flight_sagas().await;

    let sagas = result.unwrap_or_else(|e| panic!("list_in_flight_sagas should succeed: {e}"));
    // Verify it returns a (possibly empty) list
    assert!(sagas.is_empty(), "stub should return empty list");
}

// Compile-time assertion: SagaCoordinator must be Send.
// This test will fail to compile if the Send bound is lost.
#[test]
fn saga_coordinator_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<SagaCoordinator>();
}
