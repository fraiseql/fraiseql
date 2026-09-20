#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
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

// Compile-time assertion: SagaCoordinator must be Send.
// This test will fail to compile if the Send bound is lost.
#[test]
fn saga_coordinator_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<SagaCoordinator>();
}
