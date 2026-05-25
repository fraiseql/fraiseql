#![allow(clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[tokio::test]
async fn test_saga_executor_creation() {
    let executor = TestSagaExecutor::new();
    assert!(executor.execution_history.is_empty());
}

#[tokio::test]
async fn test_saga_step_execution() {
    let executor = TestSagaExecutor::new();

    let step = SagaStepDef::new(1, "order-service", "orders", json!({"orderId": "123"}));

    let step_result = executor
        .execute_step("saga-123", &step)
        .await
        .unwrap_or_else(|e| panic!("expected Ok from execute_step: {e}"));
    assert_eq!(step_result.step_number, 1);
    assert!(step_result.success);
    assert!(step_result.data.is_some());
}

#[tokio::test]
async fn test_saga_forward_phase() {
    let mut executor = TestSagaExecutor::new();

    let steps = vec![
        SagaStepDef::new(1, "order-service", "orders", json!({"orderId": "123"})),
        SagaStepDef::new(2, "inventory-service", "inventory", json!({"orderId": "123"})),
    ];

    let results = executor
        .execute_saga("saga-123", steps)
        .await
        .unwrap_or_else(|e| panic!("expected Ok from execute_saga: {e}"));
    assert_eq!(results.len(), 2);
    assert!(results[0].success);
    assert!(results[1].success);
}

#[tokio::test]
async fn test_saga_lifo_compensation() {
    let executor = TestSagaExecutor::new();

    let forward_steps = vec![
        SagaStepDef::new(1, "order-service", "orders", json!({})).with_compensation("cancelOrder"),
        SagaStepDef::new(2, "inventory-service", "inventory", json!({}))
            .with_compensation("restoreInventory"),
        SagaStepDef::new(3, "payment-service", "payments", json!({}))
            .with_compensation("refundPayment"),
    ];

    let compensation_steps = vec![
        SagaStepResult::success(3, json!({})),
        SagaStepResult::success(2, json!({})),
        SagaStepResult::success(1, json!({})),
    ];

    // Verify LIFO order
    executor
        .verify_lifo_order(&forward_steps, &compensation_steps)
        .unwrap_or_else(|e| panic!("expected Ok from verify_lifo_order: {e}"));
}
