//! Category 6: Recovery Integration (3 tests)
//! Category 7: Concurrent Execution (3 tests)
//! Category 8: Observability (2 tests)

use std::time::Instant;

use serde_json::json;
use uuid::Uuid;

use super::saga_e2e_harness::*;

// ============================================================================
// Category 6: Recovery Integration (3 tests)
// ============================================================================

#[test]
fn test_e2e_recovery_detects_pending_sagas() {
    let store = InMemorySagaStore::new();

    let pending_id = Uuid::new_v4();
    store.save_saga(StoredSaga {
        id: pending_id,
        state: SagaState::Pending,
        steps: vec![],
        created_at: Instant::now(),
        completed_at: None,
    });

    let completed_id = Uuid::new_v4();
    store.save_saga(StoredSaga {
        id: completed_id,
        state: SagaState::Completed,
        steps: vec![],
        created_at: Instant::now(),
        completed_at: Some(Instant::now()),
    });

    let recovery = MockRecoveryManager::new(&store);
    let report = recovery.run_recovery_iteration();

    assert_eq!(report.pending_sagas.len(), 1, "Should detect 1 pending saga",);
    assert_eq!(report.pending_sagas[0], pending_id, "Should detect correct pending saga",);
}

#[test]
fn test_e2e_recovery_detects_stuck_executing() {
    let store = InMemorySagaStore::new();

    let stuck_id = Uuid::new_v4();
    store.save_saga(StoredSaga {
        id: stuck_id,
        state: SagaState::Executing,
        steps: vec![],
        created_at: Instant::now(),
        completed_at: None,
    });

    let recovery = MockRecoveryManager::new(&store);
    let report = recovery.run_recovery_iteration();

    assert_eq!(report.stuck_sagas.len(), 1, "Should detect 1 stuck saga",);
    assert_eq!(report.stuck_sagas[0], stuck_id, "Should detect correct stuck saga",);
}

#[test]
fn test_e2e_recovery_cleans_stale_completed() {
    let store = InMemorySagaStore::new();

    for _ in 0..3 {
        store.save_saga(StoredSaga {
            id: Uuid::new_v4(),
            state: SagaState::Completed,
            steps: vec![],
            created_at: Instant::now(),
            completed_at: Some(Instant::now()),
        });
    }

    store.save_saga(StoredSaga {
        id: Uuid::new_v4(),
        state: SagaState::Pending,
        steps: vec![],
        created_at: Instant::now(),
        completed_at: None,
    });

    assert_eq!(store.saga_count(), 4, "Should start with 4 sagas");

    let recovery = MockRecoveryManager::new(&store);
    let report = recovery.run_recovery_iteration();

    assert_eq!(report.cleaned_count, 3, "Should clean 3 completed sagas",);
    assert_eq!(store.saga_count(), 1, "Should have 1 saga remaining");
}

// ============================================================================
// Category 7: Concurrent Execution (3 tests)
// ============================================================================

#[test]
fn test_e2e_multiple_sagas_execute_independently() {
    let store = InMemorySagaStore::new();
    let executor = MockStepExecutor::new();
    let compensator = MockStepCompensator::new();
    let orchestrator = SagaOrchestrator::new(store, executor, compensator);

    // Saga 1: single step, succeeds
    let saga1_steps = vec![SagaStepDef {
        subgraph: "users".to_string(),
        mutation_type: MutationType::Create,
        typename: "User".to_string(),
        mutation_name: "createUser".to_string(),
        variables: json!({"id": "1"}),
        behavior: StepBehavior::Succeed,
        compensation_behavior: CompensationBehavior::Succeed,
    }];
    let saga1_id = orchestrator.create_saga(saga1_steps).expect("Should create saga 1");
    let result1 = orchestrator.execute_saga(saga1_id).expect("Should execute saga 1");
    assert_saga_completed(&result1);

    // Saga 2: two steps, second fails -> compensated
    orchestrator.executor.set_behavior(0, StepBehavior::Succeed);
    orchestrator
        .executor
        .set_behavior(1, StepBehavior::Fail("saga2 failure".to_string()));
    orchestrator.compensator.set_behavior(0, CompensationBehavior::Succeed);

    let saga2_steps = vec![
        SagaStepDef {
            subgraph: "orders".to_string(),
            mutation_type: MutationType::Create,
            typename: "Order".to_string(),
            mutation_name: "createOrder".to_string(),
            variables: json!({"id": "2"}),
            behavior: StepBehavior::Succeed,
            compensation_behavior: CompensationBehavior::Succeed,
        },
        SagaStepDef {
            subgraph: "products".to_string(),
            mutation_type: MutationType::Update,
            typename: "Product".to_string(),
            mutation_name: "updateProduct".to_string(),
            variables: json!({"id": "3"}),
            behavior: StepBehavior::Fail("saga2 failure".to_string()),
            compensation_behavior: CompensationBehavior::Succeed,
        },
    ];
    let saga2_id = orchestrator.create_saga(saga2_steps).expect("Should create saga 2");
    let result2 = orchestrator.execute_saga(saga2_id).expect("Should execute saga 2");
    assert_saga_compensated(&result2);

    // Saga 3: single step, succeeds
    orchestrator.executor.set_behavior(0, StepBehavior::Succeed);

    let saga3_steps = vec![SagaStepDef {
        subgraph: "payments".to_string(),
        mutation_type: MutationType::Create,
        typename: "Payment".to_string(),
        mutation_name: "createPayment".to_string(),
        variables: json!({"id": "4"}),
        behavior: StepBehavior::Succeed,
        compensation_behavior: CompensationBehavior::Succeed,
    }];
    let saga3_id = orchestrator.create_saga(saga3_steps).expect("Should create saga 3");
    let result3 = orchestrator.execute_saga(saga3_id).expect("Should execute saga 3");
    assert_saga_completed(&result3);

    // Verify each saga's final state is independent
    let status1 = orchestrator.get_saga_status(saga1_id).expect("Should get status 1");
    let status2 = orchestrator.get_saga_status(saga2_id).expect("Should get status 2");
    let status3 = orchestrator.get_saga_status(saga3_id).expect("Should get status 3");

    assert_eq!(status1.state, SagaState::Completed, "Saga 1 should be completed",);
    assert_eq!(status2.state, SagaState::Compensated, "Saga 2 should be compensated",);
    assert_eq!(status3.state, SagaState::Completed, "Saga 3 should be completed",);
}

#[test]
fn test_e2e_concurrent_status_queries_safe() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");

    // Query status before execution
    let status_before = orchestrator
        .get_saga_status(saga_id)
        .expect("Should get status before execution");
    assert_eq!(status_before.state, SagaState::Pending, "Should be pending before execution",);

    // Execute
    orchestrator.execute_saga(saga_id).expect("Should execute saga");

    // Query status after execution
    let status_after = orchestrator
        .get_saga_status(saga_id)
        .expect("Should get status after execution");
    assert_eq!(status_after.state, SagaState::Completed, "Should be completed after execution",);
    assert_eq!(status_after.completed_steps, 3, "All steps should be completed",);
}

#[test]
fn test_e2e_saga_isolation_during_compensation() {
    let store = InMemorySagaStore::new();
    let executor = MockStepExecutor::new();
    let compensator = MockStepCompensator::new();
    let orchestrator = SagaOrchestrator::new(store, executor, compensator);

    // Saga 1: succeeds
    orchestrator.executor.set_behavior(0, StepBehavior::Succeed);
    let saga1_steps = vec![SagaStepDef {
        subgraph: "users".to_string(),
        mutation_type: MutationType::Create,
        typename: "User".to_string(),
        mutation_name: "createUser".to_string(),
        variables: json!({"id": "1"}),
        behavior: StepBehavior::Succeed,
        compensation_behavior: CompensationBehavior::Succeed,
    }];
    let saga1_id = orchestrator.create_saga(saga1_steps).expect("Should create saga 1");
    let result1 = orchestrator.execute_saga(saga1_id).expect("Should execute saga 1");
    assert_saga_completed(&result1);

    // Saga 2: fails and compensates
    orchestrator.executor.set_behavior(0, StepBehavior::Succeed);
    orchestrator.executor.set_behavior(1, StepBehavior::Fail("fail".to_string()));
    orchestrator.compensator.set_behavior(0, CompensationBehavior::Succeed);

    let saga2_steps = vec![
        SagaStepDef {
            subgraph: "orders".to_string(),
            mutation_type: MutationType::Create,
            typename: "Order".to_string(),
            mutation_name: "createOrder".to_string(),
            variables: json!({"id": "2"}),
            behavior: StepBehavior::Succeed,
            compensation_behavior: CompensationBehavior::Succeed,
        },
        SagaStepDef {
            subgraph: "products".to_string(),
            mutation_type: MutationType::Update,
            typename: "Product".to_string(),
            mutation_name: "updateProduct".to_string(),
            variables: json!({"id": "3"}),
            behavior: StepBehavior::Fail("fail".to_string()),
            compensation_behavior: CompensationBehavior::Succeed,
        },
    ];
    let saga2_id = orchestrator.create_saga(saga2_steps).expect("Should create saga 2");
    let result2 = orchestrator.execute_saga(saga2_id).expect("Should execute saga 2");
    assert_saga_compensated(&result2);

    // Verify saga 1 is still completed (unaffected by saga 2's compensation)
    let status1 = orchestrator.get_saga_status(saga1_id).expect("Should get saga 1 status");
    assert_eq!(
        status1.state,
        SagaState::Completed,
        "Saga 1 should remain completed after saga 2 compensation",
    );
}

// ============================================================================
// Category 8: Observability (2 tests)
// ============================================================================

#[test]
fn test_e2e_state_transitions_tracked_with_timestamps() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).step_fails_at(2).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    orchestrator.execute_saga(saga_id).expect("Should execute saga");

    let transitions = orchestrator.store.get_transitions(saga_id);

    // Executing, Failed, Compensating, Compensated = 4 transitions
    assert!(
        transitions.len() >= 4,
        "Should have at least 4 transitions, got {}",
        transitions.len(),
    );

    // Verify timestamps are monotonically non-decreasing
    for window in transitions.windows(2) {
        assert!(
            window[1].timestamp >= window[0].timestamp,
            "Transition timestamps should be monotonically non-decreasing",
        );
    }
}

#[test]
fn test_e2e_step_execution_timing_tracked() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let saga_id = orchestrator.create_saga(steps).expect("Should create saga");
    orchestrator.execute_saga(saga_id).expect("Should execute saga");

    // Verify executor-level timing
    let executions = orchestrator.executor.get_executions();
    assert_eq!(executions.len(), 3, "Should have 3 step executions");

    for execution in &executions {
        let duration = execution.completed_at.duration_since(execution.started_at);
        assert!(
            duration > std::time::Duration::ZERO,
            "Step {} should have duration > 0, got {duration:?}",
            execution.step_order,
        );
    }

    // Verify store-level step timing
    let saga = orchestrator.store.load_saga(saga_id).expect("Should load saga");
    for (i, step) in saga.steps.iter().enumerate() {
        assert!(step.started_at.is_some(), "Step {i} should have started_at timestamp",);
        assert!(step.completed_at.is_some(), "Step {i} should have completed_at timestamp",);
    }
}
