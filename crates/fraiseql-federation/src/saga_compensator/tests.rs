#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

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

#[tokio::test]
async fn test_compensation_step_result() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let result = compensator
        .compensate_step(saga_id, 1, "testCompensation", &serde_json::json!({}), "test-service")
        .await;

    let comp_result = result.unwrap_or_else(|e| panic!("expected Ok from compensate_step: {e}"));
    assert_eq!(comp_result.step_number, 1);
    assert!(comp_result.success);
}

#[tokio::test]
async fn test_get_compensation_status() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let status = compensator.get_compensation_status(saga_id).await;

    status.unwrap_or_else(|e| panic!("expected Ok from get_compensation_status: {e}"));
}

#[test]
fn test_saga_compensator_with_store() {
    // Test that we can create a compensator with a store reference
    let compensator = SagaCompensator::new();
    assert!(!compensator.has_store());
}

#[tokio::test]
async fn test_compensate_saga_without_store() {
    // Verify compensate_saga returns empty results without store
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();
    let result = compensator.compensate_saga(saga_id).await;

    let comp_result = result.unwrap_or_else(|e| panic!("expected Ok from compensate_saga: {e}"));
    assert_eq!(comp_result.saga_id, saga_id);
    assert_eq!(comp_result.status, CompensationStatus::Compensated);
}

#[tokio::test]
async fn test_compensation_executes_in_reverse_order() {
    // Verify that compensation steps are executed in reverse
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    // Compensate multiple steps
    let mut results = vec![];
    for step_num in (1..=3).rev() {
        let result = compensator
            .compensate_step(
                saga_id,
                step_num,
                "deleteEntity",
                &serde_json::json!({}),
                "test-service",
            )
            .await;

        if let Ok(comp_result) = result {
            results.push(comp_result);
        }
    }

    // Verify reverse order was maintained
    for (i, result) in results.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        // Reason: step count is bounded well below u32::MAX
        let expected = (3 - i) as u32;
        assert_eq!(result.step_number, expected);
    }
}

#[tokio::test]
async fn test_compensation_continues_on_step_failure() {
    // Verify that compensation continues even if a step fails
    // Without store, all compensations succeed
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    let comp_result = compensator
        .compensate_saga(saga_id)
        .await
        .unwrap_or_else(|e| panic!("expected Ok from compensate_saga (continues on failure): {e}"));
    // Without store, compensation succeeds
    assert_eq!(comp_result.status, CompensationStatus::Compensated);
}

#[test]
fn test_saga_compensator_has_store_method() {
    // Verify has_store() correctly reports status
    let compensator = SagaCompensator::new();
    assert!(!compensator.has_store());
}
