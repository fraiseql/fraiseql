//! Category 4: State Machine Validation (4 tests)
//! Category 5: Component Integration (3 tests)

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test timing assertions cast u128→f64 for threshold comparison
use super::saga_e2e_harness::*;

// ============================================================================
// Category 4: State Machine Validation (4 tests)
// ============================================================================

#[test]
fn test_e2e_success_state_transitions() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");

    // Verify initial state
    let status = orchestrator.get_saga_status(saga_id).expect("Should get status");
    assert_eq!(status.state, SagaState::Pending, "Initial state should be Pending",);

    orchestrator.execute_saga(saga_id).expect("Should execute saga");

    let transitions = orchestrator.store.get_transitions(saga_id);
    let states: Vec<&SagaState> = transitions.iter().map(|t| &t.to).collect();

    // Pending -> Executing -> Completed
    assert!(states.contains(&&SagaState::Executing), "Should have Executing transition",);
    assert!(states.contains(&&SagaState::Completed), "Should have Completed transition",);
}

#[test]
fn test_e2e_failure_compensation_state_transitions() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).step_fails_at(2).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    orchestrator.execute_saga(saga_id).expect("Should execute saga");

    let transitions = orchestrator.store.get_transitions(saga_id);
    let states: Vec<&SagaState> = transitions.iter().map(|t| &t.to).collect();

    // Pending -> Executing -> Failed -> Compensating -> Compensated
    assert!(states.contains(&&SagaState::Executing), "Should have Executing",);
    assert!(states.contains(&&SagaState::Failed), "Should have Failed");
    assert!(states.contains(&&SagaState::Compensating), "Should have Compensating",);
    assert!(states.contains(&&SagaState::Compensated), "Should have Compensated",);
}

#[test]
fn test_e2e_terminal_state_completed_immutable() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(2).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    orchestrator.execute_saga(saga_id).expect("Should execute saga");

    let result = orchestrator.execute_saga(saga_id);
    assert!(result.is_err(), "Should reject re-execution of completed saga",);
    assert!(
        result.unwrap_err().contains("completed"),
        "Error should mention completed state",
    );
}

#[test]
fn test_e2e_terminal_state_compensated_immutable() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).step_fails_at(2).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    orchestrator.execute_saga(saga_id).expect("Should execute saga");

    let result = orchestrator.execute_saga(saga_id);
    assert!(result.is_err(), "Should reject re-execution of compensated saga",);
    assert!(
        result.unwrap_err().contains("compensated"),
        "Error should mention compensated state",
    );
}

// ============================================================================
// Category 5: Component Integration (3 tests)
// ============================================================================

#[test]
fn test_e2e_coordinator_delegates_to_executor() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(2).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    orchestrator.execute_saga(saga_id).expect("Should execute saga");

    let executions = orchestrator.executor.get_executions();
    assert_eq!(executions.len(), 2, "Executor should receive 2 calls");

    assert_eq!(executions[0].subgraph, "users", "Step 0 should target 'users' subgraph",);
    assert_eq!(
        executions[0].mutation_name, "createUser",
        "Step 0 should have correct mutation name",
    );

    assert_eq!(executions[1].subgraph, "orders", "Step 1 should target 'orders' subgraph",);
    assert_eq!(
        executions[1].mutation_name, "createOrder",
        "Step 1 should have correct mutation name",
    );
}

#[test]
fn test_e2e_executor_failure_triggers_compensator() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).step_fails_at(2).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Should execute saga");

    let compensations = orchestrator.compensator.get_compensations();
    assert_eq!(compensations.len(), 2, "Compensator should be called for 2 completed steps",);

    // Compensation for step 1 (first in reverse order) should have step 1's result
    assert!(
        compensations[0].original_result.is_some(),
        "Compensation for step 1 should include original result data",
    );

    // Compensation for step 0 should have step 0's result
    assert!(
        compensations[1].original_result.is_some(),
        "Compensation for step 0 should include original result data",
    );

    assert!(result.error.is_some(), "SagaResult should include error from failed step",);
}

#[test]
fn test_e2e_compensator_results_in_saga_result() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).step_fails_at(2).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Should execute saga");

    assert_saga_compensated(&result);

    assert_eq!(
        result.compensation_results.len(),
        2,
        "SagaResult should include 2 compensation results",
    );
    for comp in &result.compensation_results {
        assert!(
            comp.result.is_ok(),
            "Each compensation result should be successful for step {}",
            comp.step_order,
        );
    }
}
