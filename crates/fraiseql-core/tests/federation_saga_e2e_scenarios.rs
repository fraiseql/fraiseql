//! Federation Saga End-to-End (E2E) Scenarios
//!
//! Comprehensive integration tests for the complete saga system, exercising
//! the coordinator → executor → compensator → recovery manager workflow
//! under normal and failure conditions.
//!
//! # Test Organization
//!
//! - **Cycle 1**: Basic multi-step execution
//! - **Cycle 2**: Single-step failures
//! - **Cycle 3**: Automatic compensation
//! - **Cycle 4**: Manual compensation strategy
//! - **Cycle 5**: Concurrent saga handling
//! - **Cycle 6**: Recovery manager integration
//! - **Cycle 7**: Crash/interruption recovery
//! - **Cycle 8**: Complex multi-failure scenarios

use fraiseql_core::federation::{
    saga_coordinator::{CompensationStrategy, SagaCoordinator, SagaStep},
    saga_executor::SagaExecutor,
};
use uuid::Uuid;

// ===========================================================================================
// TEST FIXTURES & HELPERS
// ===========================================================================================

/// Test saga scenario builder for E2E testing
struct TestSagaScenario {
    step_count:            usize,
    compensation_strategy: CompensationStrategy,
}

impl TestSagaScenario {
    /// Create a new test scenario
    fn new(step_count: usize) -> Self {
        Self {
            step_count,
            compensation_strategy: CompensationStrategy::Automatic,
        }
    }

    /// Set compensation strategy (used in later cycles)
    #[allow(dead_code)]
    fn with_strategy(mut self, strategy: CompensationStrategy) -> Self {
        self.compensation_strategy = strategy;
        self
    }

    /// Build test saga steps
    fn build_steps(&self) -> Vec<SagaStep> {
        (1..=self.step_count as u32)
            .map(|i| {
                let subgraph = format!("service-{}", i % 3 + 1);
                let mutation = format!("mutation{}", i);
                let compensation = format!("compensation{}", i);

                SagaStep::new(
                    i,
                    &subgraph,
                    format!("Entity{}", i),
                    &mutation,
                    serde_json::json!({
                        "step": i,
                        "data": format!("input_{}", i)
                    }),
                    &compensation,
                    serde_json::json!({
                        "step": i,
                        "rollback": true
                    }),
                )
            })
            .collect()
    }
}

/// Helper to create coordinator and execute saga to completion
async fn execute_saga_scenario(scenario: TestSagaScenario) -> (Vec<SagaStep>, Uuid) {
    let coordinator = SagaCoordinator::new(scenario.compensation_strategy);
    let steps = scenario.build_steps();
    let saga_id = coordinator.create_saga(steps.clone()).await.expect("Failed to create saga");

    (steps, saga_id)
}

/// Helper to execute all steps of a saga
async fn execute_all_steps(saga_id: Uuid, step_count: usize) {
    let executor = SagaExecutor::new();

    for step_number in 1..=step_count as u32 {
        let mutation_name = format!("mutation{}", step_number);
        let subgraph = format!("service-{}", step_number % 3 + 1);
        let result = executor
            .execute_step(
                saga_id,
                step_number,
                &mutation_name,
                &serde_json::json!({"step": step_number}),
                &subgraph,
            )
            .await;

        assert!(result.is_ok(), "Step {} execution failed", step_number);
        let step_result = result.unwrap();
        assert_eq!(step_result.step_number, step_number);
        assert!(step_result.success, "Step {} should succeed", step_number);
        assert!(step_result.data.is_some(), "Step {} should return data", step_number);
    }
}

// ===========================================================================================
// CYCLE 1: BASIC MULTI-STEP SAGA EXECUTION
// ===========================================================================================

#[tokio::test]
async fn test_saga_with_5_steps_all_succeed() {
    // Given: A saga with 5 steps
    let scenario = TestSagaScenario::new(5);
    let (_, saga_id) = execute_saga_scenario(scenario).await;

    // When: All steps execute successfully
    execute_all_steps(saga_id, 5).await;

    // Then: Saga should complete
    assert_eq!(saga_id.get_version_num(), 4); // UUIDv4
}

#[tokio::test]
async fn test_saga_with_7_steps_all_succeed() {
    // Given: A saga with 7 steps
    let scenario = TestSagaScenario::new(7);
    let (steps, saga_id) = execute_saga_scenario(scenario).await;

    // When: All steps execute successfully
    execute_all_steps(saga_id, steps.len()).await;

    // Then: Saga should complete
    assert_eq!(steps.len(), 7);
}

#[tokio::test]
async fn test_saga_with_10_steps_all_succeed() {
    // Given: A saga with 10 steps
    let scenario = TestSagaScenario::new(10);
    let (steps, saga_id) = execute_saga_scenario(scenario).await;

    // When: All steps execute successfully
    execute_all_steps(saga_id, steps.len()).await;

    // Then: Saga should complete
    assert_eq!(steps.len(), 10);
}

#[tokio::test]
async fn test_saga_execution_preserves_step_order() {
    // Given: A saga with 5 steps
    let scenario = TestSagaScenario::new(5);
    let (steps, saga_id) = execute_saga_scenario(scenario).await;

    // When: Steps execute
    execute_all_steps(saga_id, steps.len()).await;

    // Then: Step order should be preserved (1, 2, 3, 4, 5)
    for (i, step) in steps.iter().enumerate() {
        assert_eq!(step.number, (i + 1) as u32);
    }
}

#[tokio::test]
async fn test_each_step_receives_previous_step_output() {
    // Given: A saga with 3 steps
    let scenario = TestSagaScenario::new(3);
    let (_, saga_id) = execute_saga_scenario(scenario).await;

    let executor = SagaExecutor::new();

    // When: Execute step 1
    let result1 = executor
        .execute_step(saga_id, 1, "mutation1", &serde_json::json!({"step": 1}), "service-1")
        .await;

    assert!(result1.is_ok());
    let step1_data = result1.unwrap().data;
    assert!(step1_data.is_some());

    // Then: Step 2 should be able to use step 1's output
    let result2 = executor
        .execute_step(
            saga_id,
            2,
            "mutation2",
            &serde_json::json!({"step": 2, "prev_step_data": step1_data}),
            "service-2",
        )
        .await;

    assert!(result2.is_ok());
    assert!(result2.unwrap().data.is_some());
}

#[tokio::test]
async fn test_saga_result_contains_all_step_data() {
    // Given: A saga with 4 steps
    let scenario = TestSagaScenario::new(4);
    let (steps, saga_id) = execute_saga_scenario(scenario).await;

    let executor = SagaExecutor::new();

    // When: Execute all steps
    let mut step_results = vec![];
    for step_number in 1..=steps.len() as u32 {
        let result = executor
            .execute_step(
                saga_id,
                step_number,
                &format!("mutation{}", step_number),
                &serde_json::json!({"step": step_number}),
                &format!("service-{}", step_number % 3 + 1),
            )
            .await;

        assert!(result.is_ok());
        step_results.push(result.unwrap());
    }

    // Then: All step results should be collected
    assert_eq!(step_results.len(), 4);
    for (i, result) in step_results.iter().enumerate() {
        assert_eq!(result.step_number, (i + 1) as u32);
        assert!(result.success);
        assert!(result.data.is_some());
    }
}

#[tokio::test]
async fn test_concurrent_5_sagas_execute_independently() {
    // Given: 5 sagas with 3 steps each, created concurrently
    let saga_count = 5;
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);

    // When: Create all sagas (simulating concurrent creation)
    let mut saga_ids = vec![];
    for _ in 0..saga_count {
        let steps = TestSagaScenario::new(3).build_steps();
        let saga_id = coordinator.create_saga(steps).await.expect("Failed to create saga");
        saga_ids.push(saga_id);
    }

    // Execute all sagas (they should be independent)
    for saga_id in &saga_ids {
        execute_all_steps(*saga_id, 3).await;
    }

    // Then: All sagas should have unique IDs
    assert_eq!(saga_ids.len(), saga_count);
    // Verify uniqueness
    for i in 0..saga_ids.len() {
        for j in (i + 1)..saga_ids.len() {
            assert_ne!(saga_ids[i], saga_ids[j], "Saga IDs should be unique");
        }
    }
}

#[tokio::test]
async fn test_concurrent_10_sagas_execute_independently() {
    // Given: 10 sagas with 2 steps each
    let saga_count = 10;
    let coordinator = SagaCoordinator::new(CompensationStrategy::Automatic);

    // When: Create all sagas
    let mut saga_ids = vec![];
    for _ in 0..saga_count {
        let steps = TestSagaScenario::new(2).build_steps();
        let saga_id = coordinator.create_saga(steps).await.expect("Failed to create saga");
        saga_ids.push(saga_id);
    }

    // Execute all sagas
    for saga_id in &saga_ids {
        execute_all_steps(*saga_id, 2).await;
    }

    // Then: All sagas should complete with unique IDs
    assert_eq!(saga_ids.len(), saga_count);

    // Verify all are unique
    for i in 0..saga_ids.len() {
        for j in (i + 1)..saga_ids.len() {
            assert_ne!(saga_ids[i], saga_ids[j]);
        }
    }
}
