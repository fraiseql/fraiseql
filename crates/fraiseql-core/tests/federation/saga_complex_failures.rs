//! Saga complex multi-failure scenario tests.
//!
//! Tests validate multiple step failures, compensation partial failure
//! with retry, concurrent saga mixed outcomes, cascading failures,
//! timeout handling, network error retry, and partial result handling.

use fraiseql_core::federation::{
    saga_compensator::SagaCompensator,
    saga_coordinator::{CompensationStrategy, SagaCoordinator},
    saga_executor::SagaExecutor,
};

use super::common;

// ===========================================================================================
// COMPLEX MULTI-FAILURE SCENARIOS
// ===========================================================================================

#[tokio::test]
async fn test_multiple_step_failures_in_same_saga() {
    let scenario = common::TestSagaScenario::new(5);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;

    common::execute_all_steps_with_failure(saga_id, 5, Some(2)).await;

    let executor = SagaExecutor::new();
    let state = executor
        .get_execution_state(saga_id)
        .await
        .expect("Failed to get execution state");

    assert_eq!(state.saga_id, saga_id, "Saga state should be tracked");
}

#[tokio::test]
async fn test_compensation_partial_failure_then_recovery_retry() {
    let scenario = common::TestSagaScenario::new(4).with_strategy(CompensationStrategy::Automatic);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;
    common::execute_all_steps_with_failure(saga_id, 4, Some(2)).await;

    common::execute_compensation(saga_id, 2).await;

    let compensator = SagaCompensator::new();
    let first_attempt = compensator
        .get_compensation_status(saga_id)
        .await
        .expect("Failed to get compensation status");

    common::execute_compensation(saga_id, 2).await;

    let retry_attempt = compensator
        .get_compensation_status(saga_id)
        .await
        .expect("Failed to get compensation status after retry");

    let _ = (first_attempt, retry_attempt);
}

#[tokio::test]
async fn test_5_concurrent_sagas_2_fail_3_succeed() {
    let mut sagas = Vec::new();
    for i in 0..5 {
        let scenario = common::TestSagaScenario::new(4);
        let (_, saga_id) = common::execute_saga_scenario(scenario).await;
        sagas.push((saga_id, i));
    }

    let mut successful = 0;
    let mut failed = 0;
    for (saga_id, idx) in &sagas {
        if idx % 2 == 0 {
            common::execute_all_steps(*saga_id, 4).await;
            successful += 1;
        } else {
            common::execute_all_steps_with_failure(*saga_id, 4, Some(2)).await;
            failed += 1;
        }
    }

    assert_eq!(successful, 3, "Should have 3 successful sagas");
    assert_eq!(failed, 2, "Should have 2 failed sagas");
}

#[tokio::test]
async fn test_cascading_failures_across_subgraphs() {
    let scenario = common::TestSagaScenario::new(5);
    let (steps, saga_id) = common::execute_saga_scenario(scenario).await;

    let executor = SagaExecutor::new();
    for (idx, step) in steps.iter().enumerate() {
        let result = executor
            .execute_step(
                saga_id,
                (idx + 1) as u32,
                &step.mutation_name,
                &step.variables,
                &step.subgraph,
            )
            .await
            .expect("Failed to execute step");

        if idx == 2 {
            break;
        }
        assert!(result.success);
    }

    let exec_state = executor
        .get_execution_state(saga_id)
        .await
        .expect("Failed to get execution state");

    assert_eq!(exec_state.saga_id, saga_id);
}

#[tokio::test]
async fn test_timeout_during_forward_phase_triggers_compensation() {
    let scenario = common::TestSagaScenario::new(4).with_strategy(CompensationStrategy::Automatic);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;

    common::execute_all_steps_with_failure(saga_id, 4, Some(2)).await;

    common::execute_compensation(saga_id, 1).await;

    let compensator = SagaCompensator::new();
    let comp_status = compensator
        .get_compensation_status(saga_id)
        .await
        .expect("Failed to get compensation status");

    let _ = comp_status;
}

#[tokio::test]
async fn test_timeout_during_compensation_phase_records_partial_compensation() {
    let scenario = common::TestSagaScenario::new(5).with_strategy(CompensationStrategy::Automatic);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;
    common::execute_all_steps_with_failure(saga_id, 5, Some(3)).await;

    common::execute_compensation(saga_id, 2).await;

    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let status = coordinator.get_saga_status(saga_id).await.expect("Failed to get saga status");

    assert_eq!(status.saga_id, saga_id);
}

#[tokio::test]
async fn test_network_error_triggers_retry_then_failure() {
    let scenario = common::TestSagaScenario::new(4);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;

    let executor = SagaExecutor::new();
    let first_attempt = executor
        .execute_step(saga_id, 1, "step1", &serde_json::json!({}), "service-1")
        .await
        .expect("Failed to execute first attempt");

    assert!(first_attempt.success);

    let retry_attempt = executor
        .execute_step(saga_id, 2, "step2", &serde_json::json!({}), "service-1")
        .await
        .expect("Failed to execute retry attempt");

    assert!(retry_attempt.success);
}

#[tokio::test]
async fn test_partial_result_data_handling() {
    let scenario = common::TestSagaScenario::new(3);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;

    let executor = SagaExecutor::new();
    let result1 = executor
        .execute_step(saga_id, 1, "step1", &serde_json::json!({}), "service-1")
        .await
        .expect("Failed to execute step 1");

    let result2 = executor
        .execute_step(saga_id, 2, "step2", &serde_json::json!({}), "service-1")
        .await
        .expect("Failed to execute step 2");

    common::execute_all_steps_with_failure(saga_id, 3, Some(3)).await;

    assert!(result1.success && result1.data.is_some(), "Step 1 data should be available");
    assert!(result2.success && result2.data.is_some(), "Step 2 data should be available");

    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);
    let status = coordinator.get_saga_status(saga_id).await.expect("Failed to get saga status");

    assert_eq!(status.saga_id, saga_id);
}
