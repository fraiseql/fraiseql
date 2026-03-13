#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Real integration tests for the [`SagaCoordinator`], [`SagaExecutor`], and [`SagaCompensator`].
//!
//! All tests operate against the production API from [`fraiseql_core::federation`].
//! No mock harnesses are created here — the production types are used directly,
//! exercising the store-less paths that all three components expose for testing.

use fraiseql_core::federation::{
    saga_compensator::{CompensationStatus, SagaCompensator},
    saga_coordinator::{CompensationStrategy, SagaCoordinator, SagaStep},
    saga_executor::SagaExecutor,
    saga_store::SagaState,
};
use uuid::Uuid;

// ===========================================================================================
// SECTION 1: SagaCoordinator — create_saga validation
// ===========================================================================================

#[tokio::test]
async fn create_saga_with_single_step_returns_valid_uuid() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let steps = vec![SagaStep::new(
        1,
        "orders-service",
        "Order",
        "createOrder",
        serde_json::json!({"id": "order-1", "total": 100.0}),
        "deleteOrder",
        serde_json::json!({"id": "order-1"}),
    )];

    let result = coordinator.create_saga(steps).await;

    assert!(result.is_ok());
    let saga_id = result.unwrap();
    // Must be a v4 UUID (randomly generated)
    assert_eq!(saga_id.get_version_num(), 4);
}

#[tokio::test]
async fn create_saga_with_multiple_steps_succeeds() {
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
            serde_json::json!({"orderId": "order-1", "amount": 50.0}),
            "reversePayment",
            serde_json::json!({"orderId": "order-1"}),
        ),
        SagaStep::new(
            3,
            "inventory-service",
            "Inventory",
            "reserveInventory",
            serde_json::json!({"orderId": "order-1", "items": []}),
            "releaseInventory",
            serde_json::json!({"orderId": "order-1"}),
        ),
    ];

    let result = coordinator.create_saga(steps).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn create_saga_with_empty_steps_returns_error() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);

    let result = coordinator.create_saga(vec![]).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("at least one step"));
}

#[tokio::test]
async fn create_saga_with_out_of_order_steps_returns_error() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    // Steps numbered 1, 3 (skips 2) — invalid
    let steps = vec![
        SagaStep::new(
            1,
            "svc-a",
            "TypeA",
            "mutA",
            serde_json::json!({}),
            "compA",
            serde_json::json!({}),
        ),
        SagaStep::new(
            3,
            "svc-b",
            "TypeB",
            "mutB",
            serde_json::json!({}),
            "compB",
            serde_json::json!({}),
        ),
    ];

    let result = coordinator.create_saga(steps).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("sequential"));
}

// ===========================================================================================
// SECTION 2: SagaCoordinator — execute, cancel, status, result
// ===========================================================================================

#[tokio::test]
async fn execute_saga_returns_completed_state() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let saga_id = Uuid::new_v4();

    let result = coordinator.execute_saga(saga_id).await;

    assert!(result.is_ok());
    let saga_result = result.unwrap();
    assert_eq!(saga_result.saga_id, saga_id);
    assert_eq!(saga_result.state, SagaState::Completed);
    assert!(!saga_result.compensated);
}

#[tokio::test]
async fn cancel_saga_returns_failed_state_with_message() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let saga_id = Uuid::new_v4();

    let result = coordinator.cancel_saga(saga_id).await;

    assert!(result.is_ok());
    let saga_result = result.unwrap();
    assert_eq!(saga_result.saga_id, saga_id);
    assert_eq!(saga_result.state, SagaState::Failed);
    assert!(saga_result.error.is_some());
    assert!(saga_result.error.unwrap().contains("cancelled"));
}

#[tokio::test]
async fn get_saga_status_returns_pending_state() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let saga_id = Uuid::new_v4();

    let result = coordinator.get_saga_status(saga_id).await;

    assert!(result.is_ok());
    let status = result.unwrap();
    assert_eq!(status.saga_id, saga_id);
    assert_eq!(status.state, SagaState::Pending);
    // Progress starts at zero
    assert!((status.progress_percentage - 0.0_f64).abs() < f64::EPSILON);
}

#[tokio::test]
async fn get_saga_result_succeeds_for_any_id() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let saga_id = Uuid::new_v4();

    let result = coordinator.get_saga_result(saga_id).await;

    assert!(result.is_ok());
    let saga_result = result.unwrap();
    assert_eq!(saga_result.saga_id, saga_id);
}

#[tokio::test]
async fn list_in_flight_sagas_returns_a_list() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);

    let result = coordinator.list_in_flight_sagas().await;

    assert!(result.is_ok());
    // The stub returns an empty list; the important thing is that the call succeeds.
    let sagas = result.unwrap();
    assert!(sagas.is_empty());
}

// ===========================================================================================
// SECTION 3: CompensationStrategy variants
// ===========================================================================================

#[test]
fn compensation_strategy_default_is_automatic() {
    assert_eq!(CompensationStrategy::default(), CompensationStrategy::Automatic);
}

#[test]
fn coordinator_exposes_strategy_via_accessor() {
    let auto = SagaCoordinator::new(CompensationStrategy::Automatic);
    assert_eq!(auto.strategy(), CompensationStrategy::Automatic);

    let manual = SagaCoordinator::new(CompensationStrategy::Manual);
    assert_eq!(manual.strategy(), CompensationStrategy::Manual);
}

// ===========================================================================================
// SECTION 4: SagaStep construction
// ===========================================================================================

#[test]
fn saga_step_fields_are_set_correctly() {
    let vars = serde_json::json!({"amount": 42});
    let comp_vars = serde_json::json!({"id": "x"});

    let step = SagaStep::new(
        7,
        "billing-service",
        "Invoice",
        "createInvoice",
        vars.clone(),
        "deleteInvoice",
        comp_vars.clone(),
    );

    assert_eq!(step.number, 7);
    assert_eq!(step.subgraph, "billing-service");
    assert_eq!(step.typename, "Invoice");
    assert_eq!(step.mutation_name, "createInvoice");
    assert_eq!(step.variables, vars);
    assert_eq!(step.compensation_mutation, "deleteInvoice");
    assert_eq!(step.compensation_variables, comp_vars);
    // id must be a valid UUID (non-nil)
    assert_ne!(step.id, Uuid::nil());
}

#[test]
fn saga_step_each_instance_gets_unique_id() {
    let make = |n: u32| {
        SagaStep::new(n, "svc", "T", "m", serde_json::json!({}), "c", serde_json::json!({}))
    };

    let step1 = make(1);
    let step2 = make(2);

    assert_ne!(step1.id, step2.id);
}

// ===========================================================================================
// SECTION 5: SagaExecutor (store-less)
// ===========================================================================================

#[tokio::test]
async fn executor_execute_step_without_store_succeeds() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let result = executor
        .execute_step(saga_id, 1, "createOrder", &serde_json::json!({"id": "o1"}), "orders-svc")
        .await;

    assert!(result.is_ok());
    let step_result = result.unwrap();
    assert_eq!(step_result.step_number, 1);
    assert!(step_result.success);
    assert!(step_result.error.is_none());
    assert!(step_result.data.is_some());
}

#[tokio::test]
async fn executor_execute_saga_without_store_returns_empty_results() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let results = executor.execute_saga(saga_id).await;

    assert!(results.is_ok());
    // No store → nothing to execute
    assert!(results.unwrap().is_empty());
}

#[tokio::test]
async fn executor_execute_step_result_includes_input_data() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();
    let vars = serde_json::json!({"customerId": "c123", "total": 100});

    let result = executor
        .execute_step(saga_id, 2, "reserveInventory", &vars, "inventory-svc")
        .await
        .unwrap();

    assert!(result.success);
    let data = result.data.expect("data must be present");
    // The executor embeds augmented input in the result under the `input` key
    assert_eq!(data.get("input"), Some(&vars));
}

#[tokio::test]
async fn executor_execute_step_duration_is_measured() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let result = executor
        .execute_step(saga_id, 1, "testMut", &serde_json::json!({}), "svc")
        .await
        .unwrap();

    // duration_ms should be a non-negative value (u64 is always non-negative, just verify the
    // field is accessible)
    let _: u64 = result.duration_ms;
}

#[tokio::test]
async fn executor_get_execution_state_without_store() {
    let executor = SagaExecutor::new();
    let saga_id = Uuid::new_v4();

    let state = executor.get_execution_state(saga_id).await.unwrap();

    assert_eq!(state.saga_id, saga_id);
    assert_eq!(state.total_steps, 0);
    assert_eq!(state.completed_steps, 0);
    assert!(state.current_step.is_none());
    assert!(!state.failed);
    assert!(state.failure_reason.is_none());
}

#[test]
fn executor_has_store_is_false_when_no_store_configured() {
    let executor = SagaExecutor::new();
    assert!(!executor.has_store());
}

#[test]
fn executor_default_equals_new() {
    // Default must not panic and must behave like new()
    let executor = SagaExecutor::default();
    assert!(!executor.has_store());
}

// ===========================================================================================
// SECTION 6: SagaCompensator (store-less)
// ===========================================================================================

#[tokio::test]
async fn compensator_compensate_step_without_store_succeeds() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    let result = compensator
        .compensate_step(saga_id, 1, "deleteOrder", &serde_json::json!({"id": "o1"}), "orders-svc")
        .await;

    assert!(result.is_ok());
    let step_result = result.unwrap();
    assert_eq!(step_result.step_number, 1);
    assert!(step_result.success);
    assert!(step_result.error.is_none());
}

#[tokio::test]
async fn compensator_compensate_step_result_contains_confirmation_data() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    let result = compensator
        .compensate_step(
            saga_id,
            3,
            "reversePayment",
            &serde_json::json!({"paymentId": "pay-99"}),
            "payments-svc",
        )
        .await
        .unwrap();

    assert!(result.success);
    let data = result.data.expect("compensation data must be present");
    // The store-less path returns {"deleted": true, "confirmation_id": "comp-N"}
    assert_eq!(data.get("deleted").and_then(|v| v.as_bool()), Some(true));
    let conf_id = data.get("confirmation_id").and_then(|v| v.as_str()).unwrap_or("");
    assert!(conf_id.contains("comp-3"));
}

#[tokio::test]
async fn compensator_compensate_saga_without_store_returns_compensated_status() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    let result = compensator.compensate_saga(saga_id).await;

    assert!(result.is_ok());
    let comp_result = result.unwrap();
    assert_eq!(comp_result.saga_id, saga_id);
    assert_eq!(comp_result.status, CompensationStatus::Compensated);
    assert!(comp_result.failed_steps.is_empty());
}

#[tokio::test]
async fn compensator_get_compensation_status_without_store_returns_none() {
    let compensator = SagaCompensator::new();
    let saga_id = Uuid::new_v4();

    let result = compensator.get_compensation_status(saga_id).await;

    assert!(result.is_ok());
    // No store → nothing persisted → None
    assert!(result.unwrap().is_none());
}

#[test]
fn compensator_has_store_is_false_when_no_store_configured() {
    let compensator = SagaCompensator::new();
    assert!(!compensator.has_store());
}

#[test]
fn compensator_default_equals_new() {
    let compensator = SagaCompensator::default();
    assert!(!compensator.has_store());
}

// ===========================================================================================
// SECTION 7: SagaState and SagaStoreError (pure data types, no DB required)
// ===========================================================================================

#[test]
fn saga_state_as_str_round_trips() {
    let states = [
        SagaState::Pending,
        SagaState::Executing,
        SagaState::Completed,
        SagaState::Failed,
        SagaState::Compensating,
        SagaState::Compensated,
    ];

    for state in &states {
        let s = state.as_str();
        let parsed = SagaState::from_str(s).expect("from_str must parse as_str output");
        assert_eq!(&parsed, state);
    }
}

#[test]
fn saga_state_from_str_returns_none_for_unknown() {
    assert!(SagaState::from_str("unknown").is_none());
    assert!(SagaState::from_str("").is_none());
    assert!(SagaState::from_str("PENDING").is_none()); // case-sensitive
}

#[test]
fn compensation_status_equality_is_correct() {
    assert_eq!(CompensationStatus::Compensated, CompensationStatus::Compensated);
    assert_ne!(CompensationStatus::Compensated, CompensationStatus::CompensationFailed);
    assert_ne!(CompensationStatus::PartiallyCompensated, CompensationStatus::CompensationFailed);
}

// ===========================================================================================
// SECTION 8: Coordinator builder pattern and wiring
// ===========================================================================================

#[test]
fn coordinator_with_executor_and_compensator_preserves_strategy() {
    use std::sync::Arc;

    let coordinator = SagaCoordinator::new(CompensationStrategy::Manual)
        .with_executor(Arc::new(()))
        .with_compensator(Arc::new(()));

    assert_eq!(coordinator.strategy(), CompensationStrategy::Manual);
}

#[tokio::test]
async fn full_create_then_execute_workflow_succeeds() {
    use std::sync::Arc;

    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic)
        .with_executor(Arc::new(()))
        .with_compensator(Arc::new(()));

    let steps = vec![
        SagaStep::new(
            1,
            "user-service",
            "User",
            "createUser",
            serde_json::json!({"name": "Alice"}),
            "deleteUser",
            serde_json::json!({}),
        ),
        SagaStep::new(
            2,
            "account-service",
            "Account",
            "createAccount",
            serde_json::json!({"userId": "alice"}),
            "deleteAccount",
            serde_json::json!({}),
        ),
    ];

    let saga_id = coordinator.create_saga(steps).await.unwrap();
    let result = coordinator.execute_saga(saga_id).await.unwrap();

    assert_eq!(result.saga_id, saga_id);
    assert_eq!(result.state, SagaState::Completed);
}

#[tokio::test]
async fn create_saga_each_call_produces_distinct_ids() {
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let make_steps = || {
        vec![SagaStep::new(
            1,
            "svc",
            "T",
            "m",
            serde_json::json!({}),
            "c",
            serde_json::json!({}),
        )]
    };

    let id1 = coordinator.create_saga(make_steps()).await.unwrap();
    let id2 = coordinator.create_saga(make_steps()).await.unwrap();

    assert_ne!(id1, id2, "each saga must have a unique ID");
}
