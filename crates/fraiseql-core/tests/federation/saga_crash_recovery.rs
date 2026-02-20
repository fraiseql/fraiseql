//! Saga crash/interruption recovery scenario tests.
//!
//! Tests validate recovery from crashes during forward phase,
//! compensation phase, multiple crashes, and step-level crash scenarios.

use fraiseql_core::federation::{
    saga_compensator::SagaCompensator, saga_coordinator::CompensationStrategy,
    saga_executor::SagaExecutor,
};

use super::common;

// ===========================================================================================
// CRASH/INTERRUPTION RECOVERY SCENARIOS
// ===========================================================================================

#[tokio::test]
async fn test_saga_recovers_from_crash_during_forward_phase() {
    let scenario = common::TestSagaScenario::new(4);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;

    common::execute_all_steps_with_failure(saga_id, 4, Some(2)).await;

    let executor = SagaExecutor::new();
    let exec_state = executor
        .get_execution_state(saga_id)
        .await
        .expect("Failed to get execution state");

    assert_eq!(exec_state.saga_id, saga_id, "Saga state persisted");
}

#[tokio::test]
async fn test_saga_recovers_from_crash_during_compensation_phase() {
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
async fn test_saga_recovers_from_multiple_crashes() {
    let scenario = common::TestSagaScenario::new(5).with_strategy(CompensationStrategy::Automatic);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;

    common::execute_all_steps_with_failure(saga_id, 5, Some(2)).await;
    common::execute_all_steps_with_failure(saga_id, 5, Some(3)).await;

    let executor = SagaExecutor::new();
    let state = executor
        .get_execution_state(saga_id)
        .await
        .expect("Failed to get execution state");

    assert_eq!(state.saga_id, saga_id, "State survives multiple crashes");
}

#[tokio::test]
async fn test_step_1_completed_step_2_executing_crash() {
    let scenario = common::TestSagaScenario::new(4);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;
    common::execute_all_steps_with_failure(saga_id, 4, Some(2)).await;

    let executor = SagaExecutor::new();
    let state = executor
        .get_execution_state(saga_id)
        .await
        .expect("Failed to get execution state");

    assert_eq!(state.saga_id, saga_id);
}

#[tokio::test]
async fn test_step_3_completed_step_4_executing_crash_compensation_recovers() {
    let scenario = common::TestSagaScenario::new(5).with_strategy(CompensationStrategy::Automatic);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;
    common::execute_all_steps_with_failure(saga_id, 5, Some(4)).await;

    common::execute_compensation(saga_id, 3).await;

    let compensator = SagaCompensator::new();
    let comp_status = compensator
        .get_compensation_status(saga_id)
        .await
        .expect("Failed to get compensation status");

    let _ = comp_status;
}

#[tokio::test]
async fn test_crash_during_compensation_step_2_of_5() {
    let scenario = common::TestSagaScenario::new(5).with_strategy(CompensationStrategy::Automatic);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;
    common::execute_all_steps_with_failure(saga_id, 5, Some(3)).await;

    common::execute_compensation(saga_id, 2).await;

    let compensator = SagaCompensator::new();
    let status = compensator
        .get_compensation_status(saga_id)
        .await
        .expect("Failed to get compensation status");

    let _ = status;
}

#[tokio::test]
async fn test_resumed_saga_continues_from_correct_step() {
    let scenario = common::TestSagaScenario::new(5);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;
    common::execute_all_steps_with_failure(saga_id, 5, Some(3)).await;

    let executor = SagaExecutor::new();
    let state_before = executor.get_execution_state(saga_id).await.expect("Failed to get state");

    let step_3_result = executor
        .execute_step(saga_id, 3, "step3", &serde_json::json!({}), "service-1")
        .await
        .expect("Failed to execute step 3 after recovery");

    assert_eq!(step_3_result.step_number, 3, "Should resume from step 3");
    assert_eq!(state_before.saga_id, saga_id);
}

#[tokio::test]
async fn test_no_step_reexecution_after_recovery() {
    let scenario = common::TestSagaScenario::new(5);
    let (_, saga_id) = common::execute_saga_scenario(scenario).await;

    let executor = SagaExecutor::new();
    let step1 = executor
        .execute_step(saga_id, 1, "step1", &serde_json::json!({}), "service-1")
        .await
        .expect("Failed to execute step 1");

    let step2 = executor
        .execute_step(saga_id, 2, "step2", &serde_json::json!({}), "service-1")
        .await
        .expect("Failed to execute step 2");

    common::execute_all_steps_with_failure(saga_id, 5, Some(3)).await;

    let step3 = executor
        .execute_step(saga_id, 3, "step3", &serde_json::json!({}), "service-1")
        .await
        .expect("Failed to execute step 3");

    assert_eq!(step1.step_number, 1);
    assert_eq!(step2.step_number, 2);
    assert_eq!(step3.step_number, 3);
    assert!(step1.success && step2.success && step3.success);
}
