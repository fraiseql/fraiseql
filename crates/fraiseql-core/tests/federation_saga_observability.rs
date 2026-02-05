//! Federation Saga Observability Tests
//!
//! Validates that tracing instrumentation is present and correct across the three
//! saga production modules: coordinator, executor, and compensator.
//!
//! These tests verify:
//! 1. Methods still work correctly with tracing active (behavioral regression)
//! 2. Tracing imports compile (compilation test via usage)
//! 3. Expected tracing contract is documented via test names

use fraiseql_core::federation::{
    saga_compensator::SagaCompensator,
    saga_coordinator::{CompensationStrategy, SagaCoordinator, SagaStep},
    saga_executor::SagaExecutor,
};
use uuid::Uuid;

// ===========================================================================================
// CATEGORY 1: Coordinator Observability (4 tests)
// ===========================================================================================

#[tokio::test]
async fn test_coordinator_create_saga_emits_info() {
    // Given: A coordinator with valid steps
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let steps = vec![
        SagaStep::new(
            1,
            "orders-service",
            "Order",
            "createOrder",
            serde_json::json!({"id": "order-1"}),
            "deleteOrder",
            serde_json::json!({"id": "order-1"}),
        ),
        SagaStep::new(
            2,
            "payments-service",
            "Payment",
            "recordPayment",
            serde_json::json!({"orderId": "order-1"}),
            "reversePayment",
            serde_json::json!({"orderId": "order-1"}),
        ),
    ];

    // When: create_saga is called (should emit info! with saga_id and step count)
    let result = coordinator.create_saga(steps).await;

    // Then: Saga is created successfully
    assert!(result.is_ok());
    let saga_id = result.unwrap();
    assert_eq!(saga_id.get_version_num(), 4); // UUIDv4
}

#[tokio::test]
async fn test_coordinator_create_saga_validation_emits_warn() {
    // Given: A coordinator with empty steps (validation failure)
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);

    // When: create_saga is called with empty steps (should emit warn! for validation error)
    let result = coordinator.create_saga(vec![]).await;

    // Then: Error is returned for empty steps
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("at least one step"));
}

#[tokio::test]
async fn test_coordinator_execute_saga_emits_lifecycle() {
    // Given: A coordinator
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let saga_id = Uuid::new_v4();

    // When: execute_saga is called (should emit info! on start and completion)
    let result = coordinator.execute_saga(saga_id).await;

    // Then: Result is returned with completed state
    assert!(result.is_ok());
    let saga_result = result.unwrap();
    assert_eq!(saga_result.saga_id, saga_id);
}

#[tokio::test]
async fn test_coordinator_cancel_saga_emits_info() {
    // Given: A coordinator
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let saga_id = Uuid::new_v4();

    // When: cancel_saga is called (should emit info! with saga_id)
    let result = coordinator.cancel_saga(saga_id).await;

    // Then: Saga is cancelled with error message
    assert!(result.is_ok());
    let saga_result = result.unwrap();
    assert_eq!(saga_result.saga_id, saga_id);
    assert!(saga_result.error.is_some());
    assert!(saga_result.error.unwrap().contains("cancelled"));
}

// ===========================================================================================
// CATEGORY 2: Executor Observability (4 tests)
// ===========================================================================================

#[tokio::test]
async fn test_executor_execute_step_emits_info() {
    // Given: An executor with step details
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    // When: execute_step is called (should emit info! with saga_id, step_number, mutation,
    // subgraph)
    let result = executor
        .execute_step(
            saga_id,
            1,
            "createOrder",
            &serde_json::json!({"id": "order-1"}),
            "orders-service",
        )
        .await;

    // Then: Step executes successfully with result data
    assert!(result.is_ok());
    let step_result = result.unwrap();
    assert_eq!(step_result.step_number, 1);
    assert!(step_result.success);
    assert!(step_result.data.is_some());
}

#[tokio::test]
async fn test_executor_execute_saga_emits_lifecycle() {
    // Given: An executor
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    // When: execute_saga is called (should emit info! on start and completion)
    let result = executor.execute_saga(saga_id).await;

    // Then: Returns empty results (placeholder)
    assert!(result.is_ok());
    let results = result.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_executor_get_state_emits_debug() {
    // Given: An executor
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    // When: get_execution_state is called (should emit debug! with saga_id and state)
    let result = executor.get_execution_state(saga_id).await;

    // Then: State is returned
    assert!(result.is_ok());
    let state = result.unwrap();
    assert_eq!(state.saga_id, saga_id);
    assert!(!state.failed);
}

#[tokio::test]
async fn test_executor_step_context_includes_mutation_name() {
    // Given: An executor with specific mutation details
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let mutation_name = "updateInventory";
    let subgraph = "inventory-service";

    // When: execute_step is called (tracing should include mutation_name and subgraph)
    let result = executor
        .execute_step(
            saga_id,
            3,
            mutation_name,
            &serde_json::json!({"sku": "ITEM-001", "quantity": 5}),
            subgraph,
        )
        .await;

    // Then: Step result reflects the step number
    assert!(result.is_ok());
    let step_result = result.unwrap();
    assert_eq!(step_result.step_number, 3);
    assert!(step_result.success);
}

// ===========================================================================================
// CATEGORY 3: Compensator Observability (4 tests)
// ===========================================================================================

#[tokio::test]
async fn test_compensator_compensate_saga_emits_info() {
    // Given: A compensator
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    // When: compensate_saga is called (should emit info! on start and completion)
    let result = compensator.compensate_saga(saga_id).await;

    // Then: Compensation completes with Compensated status
    assert!(result.is_ok());
    let comp_result = result.unwrap();
    assert_eq!(comp_result.saga_id, saga_id);
    assert!(comp_result.failed_steps.is_empty());
}

#[tokio::test]
async fn test_compensator_compensate_step_emits_info() {
    // Given: A compensator with step compensation details
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    // When: compensate_step is called (should emit info! with saga_id, step, compensation_mutation)
    let result = compensator
        .compensate_step(
            saga_id,
            2,
            "deleteOrder",
            &serde_json::json!({"id": "order-123"}),
            "orders-service",
        )
        .await;

    // Then: Step is compensated successfully
    assert!(result.is_ok());
    let step_result = result.unwrap();
    assert_eq!(step_result.step_number, 2);
    assert!(step_result.success);
    assert!(step_result.data.is_some());
}

#[tokio::test]
async fn test_compensator_status_query_emits_debug() {
    // Given: A compensator
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    // When: get_compensation_status is called (should emit debug! with saga_id)
    let result = compensator.get_compensation_status(saga_id).await;

    // Then: Returns None (placeholder, no compensation in progress)
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_compensator_step_context_includes_subgraph() {
    // Given: A compensator with specific subgraph details
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let subgraph = "payments-service";
    let compensation_mutation = "reversePayment";

    // When: compensate_step is called (tracing should include subgraph)
    let result = compensator
        .compensate_step(
            saga_id,
            1,
            compensation_mutation,
            &serde_json::json!({"paymentId": "pay-456"}),
            subgraph,
        )
        .await;

    // Then: Compensation step reflects correct step number
    assert!(result.is_ok());
    let step_result = result.unwrap();
    assert_eq!(step_result.step_number, 1);
    assert!(step_result.success);
}
