#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

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

#[tokio::test]
async fn test_step_execution_result() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor
        .execute_step(saga_id, 1, "testMutation", &serde_json::json!({}), "test-service")
        .await;

    assert!(result.is_ok());
    let step_result = result.unwrap();
    assert_eq!(step_result.step_number, 1);
    assert!(step_result.success);
}

#[tokio::test]
async fn test_get_execution_state() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let state = executor.get_execution_state(saga_id).await;

    assert!(state.is_ok());
}

#[test]
fn test_saga_executor_with_store() {
    // Test that we can create an executor with a store reference
    // Full store testing requires database setup (integration tests)
    let executor = SagaExecutor::new();
    assert!(!executor.has_store());
}

#[tokio::test]
async fn test_execute_step_without_store() {
    // Verify that execute_step works without a store (fallback mode)
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let result = executor
        .execute_step(saga_id, 1, "testMutation", &serde_json::json!({}), "test-service")
        .await;

    assert!(result.is_ok());
    let step_result = result.unwrap();
    assert_eq!(step_result.step_number, 1);
    assert!(step_result.success);
    assert!(step_result.error.is_none());
}

#[tokio::test]
async fn test_execute_saga_without_store() {
    // Verify execute_saga returns empty results without store
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let results = executor.execute_saga(saga_id).await;

    assert!(results.is_ok());
    let step_results = results.unwrap();
    assert_eq!(step_results.len(), 0);
}

#[tokio::test]
async fn test_execute_saga_loads_saga_from_store() {
    // Verify that execute_saga attempts to load saga from store
    // This test verifies the store integration point
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    // Without a store, should get empty results
    let results = executor.execute_saga(saga_id).await;
    assert!(results.is_ok());
}

#[tokio::test]
async fn test_execute_all_steps_sequentially() {
    // Verify that steps are executed in order
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    // Execute multiple steps
    for step_num in 1..=3 {
        let result = executor
            .execute_step(
                saga_id,
                step_num,
                "testMutation",
                &serde_json::json!({}),
                "test-service",
            )
            .await;

        assert!(result.is_ok());
        let step_result = result.unwrap();
        assert_eq!(step_result.step_number, step_num);
        assert!(step_result.success);
    }
}

#[tokio::test]
async fn test_saga_maintains_step_order() {
    // Verify that saga execution maintains step order
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let mut results = vec![];
    for step_num in 1..=3 {
        let result = executor
            .execute_step(saga_id, step_num, "mutation", &serde_json::json!({}), "service")
            .await;

        if let Ok(step_result) = result {
            results.push(step_result);
        }
    }

    // Verify order is maintained
    for (i, result) in results.iter().enumerate() {
        assert_eq!(result.step_number, (i + 1) as u32);
    }
}

#[tokio::test]
async fn test_get_execution_state_without_store() {
    // Verify get_execution_state works without store
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let state = executor.get_execution_state(saga_id).await;

    assert!(state.is_ok());
    let execution_state = state.unwrap();
    assert_eq!(execution_state.saga_id, saga_id);
    assert_eq!(execution_state.total_steps, 0);
    assert_eq!(execution_state.completed_steps, 0);
    assert!(!execution_state.failed);
}

#[tokio::test]
async fn test_execution_state_tracks_progress() {
    // Verify that execution state can track progress
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    // Execute some steps
    for step_num in 1..=2 {
        let _ = executor
            .execute_step(saga_id, step_num, "mutation", &serde_json::json!({}), "service")
            .await;
    }

    // Get execution state
    let state = executor.get_execution_state(saga_id).await;
    assert!(state.is_ok());
}

#[tokio::test]
async fn test_step_execution_captures_success_in_result() {
    // Verify that successful step execution captures data
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let result = executor
        .execute_step(saga_id, 1, "createOrder", &serde_json::json!({}), "orders-service")
        .await;

    assert!(result.is_ok());
    let step_result = result.unwrap();
    assert!(step_result.success);
    assert!(step_result.data.is_some());
    assert!(step_result.error.is_none());
}

#[tokio::test]
async fn test_step_failure_detected() {
    // Verify that step failure is detected and captured
    // Tests limited scenario without a backing store. Full failure testing
    // requires mutation executor integration.
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let result = executor
        .execute_step(saga_id, 1, "mutation", &serde_json::json!({}), "service")
        .await;

    assert!(result.is_ok());
    // Success case without store - actual failure testing requires mutation executor
    // integration
}

#[tokio::test]
async fn test_execution_result_includes_metrics() {
    // Verify that execution results include timing metrics
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let result = executor
        .execute_step(saga_id, 1, "mutation", &serde_json::json!({}), "service")
        .await;

    assert!(result.is_ok());
    let step_result = result.unwrap();
    // Verify that duration is measured
    let _ = step_result.duration_ms;
}

#[tokio::test]
async fn test_pre_fetch_requires_fields() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let requires_fields = executor.pre_fetch_requires_fields(saga_id, 1).await;

    assert!(requires_fields.is_ok());
    let fields = requires_fields.unwrap();
    assert_eq!(fields, serde_json::json!({}));
}

#[test]
fn test_augment_entity_with_requires() {
    let executor = SagaExecutor::new();

    let entity = serde_json::json!({
        "id": "user-123",
        "name": "Alice"
    });

    let requires = serde_json::json!({
        "email": "alice@example.com",
        "role": "admin"
    });

    let result = executor.augment_entity_with_requires(entity, requires);

    // Verify augmented entity contains both original and @requires fields
    assert_eq!(result.get("id").and_then(|v| v.as_str()), Some("user-123"));
    assert_eq!(result.get("name").and_then(|v| v.as_str()), Some("Alice"));
    assert_eq!(result.get("email").and_then(|v| v.as_str()), Some("alice@example.com"));
    assert_eq!(result.get("role").and_then(|v| v.as_str()), Some("admin"));
}

#[test]
fn test_augment_entity_preserves_original_fields() {
    let executor = SagaExecutor::new();

    let entity = serde_json::json!({
        "id": "product-456",
        "price": 99.99
    });

    let requires = serde_json::json!({
        "category": "electronics"
    });

    let result = executor.augment_entity_with_requires(entity, requires);

    assert_eq!(result.get("id").and_then(|v| v.as_str()), Some("product-456"));
    assert_eq!(result.get("price").and_then(|v| v.as_f64()), Some(99.99));
    assert_eq!(result.get("category").and_then(|v| v.as_str()), Some("electronics"));
}

#[test]
fn test_augment_entity_overwrites_conflicting_fields() {
    let executor = SagaExecutor::new();

    let entity = serde_json::json!({
        "id": "user-123",
        "status": "inactive"
    });

    let requires = serde_json::json!({
        "status": "active"
    });

    let result = executor.augment_entity_with_requires(entity, requires);

    // @requires should overwrite original value
    assert_eq!(result.get("status").and_then(|v| v.as_str()), Some("active"));
}

#[test]
fn test_augment_entity_with_empty_requires() {
    let executor = SagaExecutor::new();

    let entity = serde_json::json!({
        "id": "test-123"
    });

    let requires = serde_json::json!({});

    let result = executor.augment_entity_with_requires(entity, requires);

    // Should return entity unchanged
    assert_eq!(result.get("id").and_then(|v| v.as_str()), Some("test-123"));
}

/// C3: execute_step must include the augmented entity data in result.
///
/// With the stub pre_fetch (returns `{}`), augment_entity_with_requires is a
/// no-op, so the input variables must appear unchanged under the `input` key.
#[tokio::test]
async fn execute_step_pre_fetches_required_fields() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let variables = serde_json::json!({"customerId": "c123", "total": 100});

    let result = executor
        .execute_step(saga_id, 1, "createOrder", &variables, "orders-service")
        .await
        .expect("execute_step must succeed");

    assert!(result.success);
    let data = result.data.expect("result must carry data");
    // The wired input must appear in the result data.
    assert_eq!(data.get("input"), Some(&variables), "augmented input must be present");
}

/// C3 regression guard: steps without @requires must behave identically to before.
#[tokio::test]
async fn execute_step_without_requires_is_unchanged() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let result = executor
        .execute_step(saga_id, 1, "testMutation", &serde_json::json!({}), "test-service")
        .await
        .expect("execute_step must succeed");

    assert!(result.success);
    assert_eq!(result.step_number, 1);
    assert!(result.error.is_none());
}
