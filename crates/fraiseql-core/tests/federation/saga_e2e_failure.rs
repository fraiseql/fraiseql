//! Category 2: Failure and Compensation (4 tests)
//! Category 3: Compensation Edge Cases (3 tests)

use super::saga_e2e_harness::*;

// ============================================================================
// Category 2: Failure and Compensation (4 tests)
// ============================================================================

#[test]
fn test_e2e_step_2_fails_compensates_step_1() {
    // Step at index 1 (second step) fails; compensate index 0 only
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).step_fails_at(1).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Should execute saga");

    assert_saga_compensated(&result);
    assert_eq!(result.completed_steps, 1, "Only step 0 should complete");

    let compensations = orchestrator.compensator.get_compensations();
    assert_eq!(compensations.len(), 1, "Should compensate 1 step");
    assert_eq!(compensations[0].step_order, 0, "Should compensate step 0",);
}

#[test]
fn test_e2e_step_4_fails_compensates_3_2_1_in_reverse() {
    // 5-step saga, step at index 3 (fourth step) fails
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(5).step_fails_at(3).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Should execute saga");

    assert_saga_compensated(&result);
    assert_eq!(result.completed_steps, 3, "Steps 0-2 should complete");

    let compensations = orchestrator.compensator.get_compensations();
    assert_eq!(compensations.len(), 3, "Should compensate 3 steps");

    let comp_order: Vec<usize> = compensations.iter().map(|c| c.step_order).collect();
    assert_eq!(comp_order, vec![2, 1, 0], "Compensation should be in reverse order",);
}

#[test]
fn test_e2e_first_step_fails_no_compensation_needed() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).step_fails_at(0).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Should execute saga");

    assert_saga_compensated(&result);
    assert_eq!(result.completed_steps, 0, "No steps should complete");

    let compensations = orchestrator.compensator.get_compensations();
    assert_eq!(compensations.len(), 0, "No compensation needed when first step fails",);
}

#[test]
fn test_e2e_last_step_fails_all_previous_compensated() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(4).step_fails_at(3).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Should execute saga");

    assert_saga_compensated(&result);
    assert_eq!(result.completed_steps, 3, "Steps 0-2 should complete");

    let compensations = orchestrator.compensator.get_compensations();
    assert_eq!(compensations.len(), 3, "All 3 completed steps should be compensated",);

    let comp_order: Vec<usize> = compensations.iter().map(|c| c.step_order).collect();
    assert_eq!(comp_order, vec![2, 1, 0], "Compensation should be in reverse order",);
}

// ============================================================================
// Category 3: Compensation Edge Cases (3 tests)
// ============================================================================

#[test]
fn test_e2e_compensation_partially_fails() {
    // Step 2 (index 2) fails; compensation of step 1 fails, step 0 succeeds
    let (orchestrator, steps) = OrchestratorBuilder::new()
        .with_steps(3)
        .step_fails_at(2)
        .compensation_fails_at(1)
        .build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Should execute saga");

    assert_saga_compensation_failed(&result);

    let compensations = orchestrator.compensator.get_compensations();
    assert_eq!(compensations.len(), 2, "Should attempt 2 compensations");

    // Reverse order: step 1 first, then step 0
    assert!(compensations[0].result.is_err(), "Step 1 compensation should fail",);
    assert!(compensations[1].result.is_ok(), "Step 0 compensation should succeed",);
}

#[test]
fn test_e2e_all_compensations_fail() {
    let (orchestrator, steps) = OrchestratorBuilder::new()
        .with_steps(3)
        .step_fails_at(2)
        .all_compensations_fail()
        .build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Should execute saga");

    assert_saga_compensation_failed(&result);

    let compensations = orchestrator.compensator.get_compensations();
    assert_eq!(compensations.len(), 2, "Should attempt compensation for 2 completed steps",);
    for comp in &compensations {
        assert!(comp.result.is_err(), "Compensation should fail for step {}", comp.step_order,);
    }
}

#[test]
fn test_e2e_compensation_idempotency() {
    // Run the same saga pattern twice; both should produce consistent results
    let results: Vec<SagaResult> = (0..2)
        .map(|_| {
            let (orchestrator, steps) =
                OrchestratorBuilder::new().with_steps(3).step_fails_at(2).build();

            let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
            orchestrator.execute_saga(saga_id).expect("Should execute saga")
        })
        .collect();

    for result in &results {
        assert_saga_compensated(result);
    }

    assert_eq!(
        results[0].compensation_results.len(),
        results[1].compensation_results.len(),
        "Repeated compensation should produce consistent number of results",
    );
    assert_eq!(
        results[0].completed_steps, results[1].completed_steps,
        "Repeated compensation should produce consistent completed step count",
    );
}
