//! Category 1: Full Saga Lifecycle (3 tests)

use super::saga_e2e_harness::*;

#[test]
fn test_e2e_three_step_saga_completes_successfully() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Should execute saga");

    assert_saga_completed(&result);
    assert_eq!(result.total_steps, 3, "Should have 3 steps");
}

#[test]
fn test_e2e_five_step_saga_completes_successfully() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(5).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Should execute saga");

    assert_saga_completed(&result);
    assert_eq!(result.total_steps, 5, "Should have 5 steps");

    // Verify execution order
    let executions = orchestrator.executor.get_executions();
    let order: Vec<usize> = executions.iter().map(|e| e.step_order).collect();
    assert_eq!(order, vec![0, 1, 2, 3, 4], "Steps should execute in order 0-4",);
}

#[test]
fn test_e2e_step_results_chained_to_next_step() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Should execute saga");

    assert_saga_completed(&result);

    let executions = orchestrator.executor.get_executions();
    assert_eq!(executions.len(), 3, "Should have 3 executions");

    // Step 0 should have no previous result
    assert!(executions[0].previous_result.is_none(), "Step 0 should have no previous result",);

    // Step 1 should receive step 0's result
    assert!(
        executions[1].previous_result.is_some(),
        "Step 1 should receive step 0's result as previous_result",
    );
    let step0_result = executions[0].result.as_ref().unwrap();
    let step1_prev = executions[1].previous_result.as_ref().unwrap();
    assert_eq!(
        step0_result, step1_prev,
        "Step 1's previous_result should equal step 0's result",
    );

    // Step 2 should receive step 1's result
    assert!(
        executions[2].previous_result.is_some(),
        "Step 2 should receive step 1's result as previous_result",
    );
    let step1_result = executions[1].result.as_ref().unwrap();
    let step2_prev = executions[2].previous_result.as_ref().unwrap();
    assert_eq!(
        step1_result, step2_prev,
        "Step 2's previous_result should equal step 1's result",
    );
}
