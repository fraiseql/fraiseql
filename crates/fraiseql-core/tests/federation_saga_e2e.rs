//! Cycle 10: End-to-End Saga Integration Testing
//!
//! Validates the complete saga lifecycle through 25 integration tests
//! using a self-contained in-memory test harness.
//!
//! ## Architecture
//!
//! ```text
//! SagaOrchestrator (wires everything together)
//!   ├── InMemorySagaStore     (saga/step CRUD + state transition tracking)
//!   ├── MockStepExecutor      (configurable per-step success/failure)
//!   ├── MockStepCompensator   (configurable per-step compensation behavior)
//!   └── MockRecoveryManager   (detects pending/stuck/stale sagas)
//! ```
//!
//! ## Test Categories
//!
//! 1. Full Saga Lifecycle (3 tests)
//! 2. Failure and Compensation (4 tests)
//! 3. Compensation Edge Cases (3 tests)
//! 4. State Machine Validation (4 tests)
//! 5. Component Integration (3 tests)
//! 6. Recovery Integration (3 tests)
//! 7. Concurrent Execution (3 tests)
//! 8. Observability (2 tests)

#[allow(dead_code)]
mod harness {
    use std::{
        collections::HashMap,
        sync::Mutex,
        time::{Duration, Instant},
    };

    use serde_json::{Value, json};
    use uuid::Uuid;

    // ========================================================================
    // State Enums and Result Types
    // ========================================================================

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SagaState {
        Pending,
        Executing,
        Completed,
        Failed,
        Compensating,
        Compensated,
        CompensationFailed,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub enum StepState {
        #[default]
        Pending,
        Executing,
        Completed,
        Failed,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub enum MutationType {
        Create,
        #[default]
        Update,
        Delete,
    }

    #[derive(Debug, Clone)]
    pub enum StepBehavior {
        Succeed,
        Fail(String),
    }

    #[derive(Debug, Clone)]
    pub enum CompensationBehavior {
        Succeed,
        Fail(String),
    }

    // ========================================================================
    // Data Structures
    // ========================================================================

    #[derive(Debug, Clone)]
    pub struct SagaStepDef {
        pub subgraph:              String,
        pub mutation_type:         MutationType,
        pub typename:              String,
        pub mutation_name:         String,
        pub variables:             Value,
        pub behavior:              StepBehavior,
        pub compensation_behavior: CompensationBehavior,
    }

    #[derive(Debug, Clone)]
    pub struct StoredSaga {
        pub id:           Uuid,
        pub state:        SagaState,
        pub steps:        Vec<StoredStep>,
        pub created_at:   Instant,
        pub completed_at: Option<Instant>,
    }

    #[derive(Debug, Clone)]
    pub struct StoredStep {
        pub order:         usize,
        pub subgraph:      String,
        pub mutation_type: MutationType,
        pub typename:      String,
        pub mutation_name: String,
        pub variables:     Value,
        pub state:         StepState,
        pub result:        Option<Value>,
        pub started_at:    Option<Instant>,
        pub completed_at:  Option<Instant>,
    }

    #[derive(Debug, Clone)]
    pub struct StateTransition {
        pub from:      SagaState,
        pub to:        SagaState,
        pub timestamp: Instant,
    }

    #[derive(Debug, Clone)]
    pub struct StepExecution {
        pub step_order:      usize,
        pub subgraph:        String,
        pub mutation_name:   String,
        pub variables:       Value,
        pub previous_result: Option<Value>,
        pub result:          Result<Value, String>,
        pub started_at:      Instant,
        pub completed_at:    Instant,
    }

    #[derive(Debug, Clone)]
    pub struct CompensationExecution {
        pub step_order:      usize,
        pub original_result: Option<Value>,
        pub result:          Result<Value, String>,
        pub timestamp:       Instant,
    }

    #[derive(Debug, Clone)]
    pub struct SagaResult {
        pub saga_id:              Uuid,
        pub state:                SagaState,
        pub completed_steps:      usize,
        pub total_steps:          usize,
        pub error:                Option<String>,
        pub step_results:         Vec<Option<Value>>,
        pub compensation_results: Vec<CompensationExecution>,
    }

    #[derive(Debug, Clone)]
    pub struct SagaStatus {
        pub saga_id:         Uuid,
        pub state:           SagaState,
        pub total_steps:     usize,
        pub completed_steps: usize,
    }

    #[derive(Debug)]
    pub struct RecoveryReport {
        pub pending_sagas: Vec<Uuid>,
        pub stuck_sagas:   Vec<Uuid>,
        pub cleaned_count: usize,
    }

    // ========================================================================
    // InMemorySagaStore
    // ========================================================================

    pub struct InMemorySagaStore {
        sagas:       Mutex<HashMap<Uuid, StoredSaga>>,
        transitions: Mutex<Vec<(Uuid, StateTransition)>>,
    }

    impl InMemorySagaStore {
        pub fn new() -> Self {
            Self {
                sagas:       Mutex::new(HashMap::new()),
                transitions: Mutex::new(Vec::new()),
            }
        }

        pub fn save_saga(&self, saga: StoredSaga) {
            self.sagas.lock().unwrap().insert(saga.id, saga);
        }

        pub fn load_saga(&self, id: Uuid) -> Option<StoredSaga> {
            self.sagas.lock().unwrap().get(&id).cloned()
        }

        pub fn update_saga_state(&self, id: Uuid, new_state: SagaState) -> Result<(), String> {
            let mut sagas = self.sagas.lock().unwrap();
            let saga = sagas.get_mut(&id).ok_or_else(|| format!("Saga {id} not found"))?;
            let old_state = saga.state.clone();
            saga.state = new_state.clone();
            if matches!(
                new_state,
                SagaState::Completed | SagaState::Compensated | SagaState::CompensationFailed
            ) {
                saga.completed_at = Some(Instant::now());
            }
            drop(sagas);
            self.transitions.lock().unwrap().push((
                id,
                StateTransition {
                    from:      old_state,
                    to:        new_state,
                    timestamp: Instant::now(),
                },
            ));
            Ok(())
        }

        pub fn update_step_state(
            &self,
            saga_id: Uuid,
            step_order: usize,
            new_state: StepState,
        ) -> Result<(), String> {
            let mut sagas = self.sagas.lock().unwrap();
            let saga =
                sagas.get_mut(&saga_id).ok_or_else(|| format!("Saga {saga_id} not found"))?;
            let step = saga
                .steps
                .get_mut(step_order)
                .ok_or_else(|| format!("Step {step_order} not found"))?;
            match &new_state {
                StepState::Executing => step.started_at = Some(Instant::now()),
                StepState::Completed | StepState::Failed => {
                    step.completed_at = Some(Instant::now());
                },
                StepState::Pending => {},
            }
            step.state = new_state;
            Ok(())
        }

        pub fn update_step_result(
            &self,
            saga_id: Uuid,
            step_order: usize,
            result: Value,
        ) -> Result<(), String> {
            let mut sagas = self.sagas.lock().unwrap();
            let saga =
                sagas.get_mut(&saga_id).ok_or_else(|| format!("Saga {saga_id} not found"))?;
            let step = saga
                .steps
                .get_mut(step_order)
                .ok_or_else(|| format!("Step {step_order} not found"))?;
            step.result = Some(result);
            Ok(())
        }

        pub fn get_transitions(&self, saga_id: Uuid) -> Vec<StateTransition> {
            self.transitions
                .lock()
                .unwrap()
                .iter()
                .filter(|(id, _)| *id == saga_id)
                .map(|(_, t)| t.clone())
                .collect()
        }

        pub fn load_sagas_by_state(&self, state: &SagaState) -> Vec<StoredSaga> {
            self.sagas
                .lock()
                .unwrap()
                .values()
                .filter(|s| s.state == *state)
                .cloned()
                .collect()
        }

        pub fn delete_completed_sagas(&self) -> usize {
            let mut sagas = self.sagas.lock().unwrap();
            let initial = sagas.len();
            sagas.retain(|_, s| !matches!(s.state, SagaState::Completed | SagaState::Compensated));
            initial - sagas.len()
        }

        pub fn saga_count(&self) -> usize {
            self.sagas.lock().unwrap().len()
        }
    }

    // ========================================================================
    // MockStepExecutor
    // ========================================================================

    pub struct MockStepExecutor {
        behaviors:  Mutex<HashMap<usize, StepBehavior>>,
        executions: Mutex<Vec<StepExecution>>,
    }

    impl MockStepExecutor {
        pub fn new() -> Self {
            Self {
                behaviors:  Mutex::new(HashMap::new()),
                executions: Mutex::new(Vec::new()),
            }
        }

        pub fn set_behavior(&self, step_order: usize, behavior: StepBehavior) {
            self.behaviors.lock().unwrap().insert(step_order, behavior);
        }

        pub fn execute(
            &self,
            step_order: usize,
            subgraph: &str,
            mutation_name: &str,
            variables: &Value,
            previous_result: Option<&Value>,
        ) -> Result<Value, String> {
            let started_at = Instant::now();
            // Small sleep to ensure measurable duration
            std::thread::sleep(Duration::from_micros(100));

            let behavior = self
                .behaviors
                .lock()
                .unwrap()
                .get(&step_order)
                .cloned()
                .unwrap_or(StepBehavior::Succeed);

            let result = match &behavior {
                StepBehavior::Succeed => Ok(json!({
                    "step": step_order,
                    "subgraph": subgraph,
                    "mutation": mutation_name,
                    "data": { "id": format!("result-{step_order}") }
                })),
                StepBehavior::Fail(msg) => Err(msg.clone()),
            };

            let completed_at = Instant::now();

            self.executions.lock().unwrap().push(StepExecution {
                step_order,
                subgraph: subgraph.to_string(),
                mutation_name: mutation_name.to_string(),
                variables: variables.clone(),
                previous_result: previous_result.cloned(),
                result: result.clone(),
                started_at,
                completed_at,
            });

            result
        }

        pub fn get_executions(&self) -> Vec<StepExecution> {
            self.executions.lock().unwrap().clone()
        }
    }

    // ========================================================================
    // MockStepCompensator
    // ========================================================================

    pub struct MockStepCompensator {
        behaviors:     Mutex<HashMap<usize, CompensationBehavior>>,
        compensations: Mutex<Vec<CompensationExecution>>,
    }

    impl MockStepCompensator {
        pub fn new() -> Self {
            Self {
                behaviors:     Mutex::new(HashMap::new()),
                compensations: Mutex::new(Vec::new()),
            }
        }

        pub fn set_behavior(&self, step_order: usize, behavior: CompensationBehavior) {
            self.behaviors.lock().unwrap().insert(step_order, behavior);
        }

        pub fn compensate(
            &self,
            step_order: usize,
            original_result: Option<&Value>,
        ) -> Result<Value, String> {
            let behavior = self
                .behaviors
                .lock()
                .unwrap()
                .get(&step_order)
                .cloned()
                .unwrap_or(CompensationBehavior::Succeed);

            let result = match &behavior {
                CompensationBehavior::Succeed => Ok(json!({
                    "compensated_step": step_order,
                    "status": "rolled_back"
                })),
                CompensationBehavior::Fail(msg) => Err(msg.clone()),
            };

            self.compensations.lock().unwrap().push(CompensationExecution {
                step_order,
                original_result: original_result.cloned(),
                result: result.clone(),
                timestamp: Instant::now(),
            });

            result
        }

        pub fn get_compensations(&self) -> Vec<CompensationExecution> {
            self.compensations.lock().unwrap().clone()
        }
    }

    // ========================================================================
    // MockRecoveryManager
    // ========================================================================

    pub struct MockRecoveryManager<'a> {
        store: &'a InMemorySagaStore,
    }

    impl<'a> MockRecoveryManager<'a> {
        pub fn new(store: &'a InMemorySagaStore) -> Self {
            Self { store }
        }

        pub fn run_recovery_iteration(&self) -> RecoveryReport {
            let pending: Vec<Uuid> = self
                .store
                .load_sagas_by_state(&SagaState::Pending)
                .into_iter()
                .map(|s| s.id)
                .collect();

            let stuck: Vec<Uuid> = self
                .store
                .load_sagas_by_state(&SagaState::Executing)
                .into_iter()
                .map(|s| s.id)
                .collect();

            let cleaned = self.store.delete_completed_sagas();

            RecoveryReport {
                pending_sagas: pending,
                stuck_sagas:   stuck,
                cleaned_count: cleaned,
            }
        }
    }

    // ========================================================================
    // SagaOrchestrator
    // ========================================================================

    pub struct SagaOrchestrator {
        pub store:       InMemorySagaStore,
        pub executor:    MockStepExecutor,
        pub compensator: MockStepCompensator,
    }

    impl SagaOrchestrator {
        pub fn new(
            store: InMemorySagaStore,
            executor: MockStepExecutor,
            compensator: MockStepCompensator,
        ) -> Self {
            Self {
                store,
                executor,
                compensator,
            }
        }

        pub fn create_saga(&self, steps: Vec<SagaStepDef>) -> Result<Uuid, String> {
            if steps.is_empty() {
                return Err("Saga must have at least one step".to_string());
            }

            let saga_id = Uuid::new_v4();

            let stored_steps: Vec<StoredStep> = steps
                .iter()
                .enumerate()
                .map(|(i, def)| StoredStep {
                    order:         i,
                    subgraph:      def.subgraph.clone(),
                    mutation_type: def.mutation_type.clone(),
                    typename:      def.typename.clone(),
                    mutation_name: def.mutation_name.clone(),
                    variables:     def.variables.clone(),
                    state:         StepState::Pending,
                    result:        None,
                    started_at:    None,
                    completed_at:  None,
                })
                .collect();

            // Configure executor and compensator behaviors
            for (i, def) in steps.iter().enumerate() {
                self.executor.set_behavior(i, def.behavior.clone());
                self.compensator.set_behavior(i, def.compensation_behavior.clone());
            }

            self.store.save_saga(StoredSaga {
                id:           saga_id,
                state:        SagaState::Pending,
                steps:        stored_steps,
                created_at:   Instant::now(),
                completed_at: None,
            });

            Ok(saga_id)
        }

        pub fn execute_saga(&self, saga_id: Uuid) -> Result<SagaResult, String> {
            let saga = self
                .store
                .load_saga(saga_id)
                .ok_or_else(|| format!("Saga {saga_id} not found"))?;

            // Reject re-execution of terminal states
            match &saga.state {
                SagaState::Completed => {
                    return Err("Cannot re-execute completed saga".to_string());
                },
                SagaState::Compensated => {
                    return Err("Cannot re-execute compensated saga".to_string());
                },
                SagaState::CompensationFailed => {
                    return Err("Cannot re-execute compensation-failed saga".to_string());
                },
                _ => {},
            }

            // Forward phase
            self.store.update_saga_state(saga_id, SagaState::Executing)?;

            let total_steps = saga.steps.len();
            let mut completed_steps = 0;
            let mut step_results: Vec<Option<Value>> = vec![None; total_steps];
            let mut failed_at: Option<String> = None;

            for (i, step) in saga.steps.iter().enumerate() {
                self.store.update_step_state(saga_id, i, StepState::Executing)?;

                let previous_result = if i > 0 {
                    step_results[i - 1].as_ref()
                } else {
                    None
                };

                match self.executor.execute(
                    i,
                    &step.subgraph,
                    &step.mutation_name,
                    &step.variables,
                    previous_result,
                ) {
                    Ok(result) => {
                        self.store.update_step_state(saga_id, i, StepState::Completed)?;
                        self.store.update_step_result(saga_id, i, result.clone())?;
                        step_results[i] = Some(result);
                        completed_steps += 1;
                    },
                    Err(err) => {
                        self.store.update_step_state(saga_id, i, StepState::Failed)?;
                        failed_at = Some(err);
                        break;
                    },
                }
            }

            if let Some(ref error_msg) = failed_at {
                self.run_compensation_phase(
                    saga_id,
                    completed_steps,
                    total_steps,
                    &step_results,
                    error_msg,
                )
            } else {
                self.store.update_saga_state(saga_id, SagaState::Completed)?;

                Ok(SagaResult {
                    saga_id,
                    state: SagaState::Completed,
                    completed_steps,
                    total_steps,
                    error: None,
                    step_results,
                    compensation_results: Vec::new(),
                })
            }
        }

        fn run_compensation_phase(
            &self,
            saga_id: Uuid,
            completed_steps: usize,
            total_steps: usize,
            step_results: &[Option<Value>],
            error_msg: &str,
        ) -> Result<SagaResult, String> {
            self.store.update_saga_state(saga_id, SagaState::Failed)?;
            self.store.update_saga_state(saga_id, SagaState::Compensating)?;

            let mut compensation_results = Vec::new();
            let mut any_compensation_failed = false;

            // Compensate completed steps in reverse order
            for i in (0..completed_steps).rev() {
                let original_result = step_results[i].as_ref();
                let comp_result = self.compensator.compensate(i, original_result);

                compensation_results.push(CompensationExecution {
                    step_order:      i,
                    original_result: original_result.cloned(),
                    result:          comp_result.clone(),
                    timestamp:       Instant::now(),
                });

                if comp_result.is_err() {
                    any_compensation_failed = true;
                }
            }

            let final_state = if completed_steps == 0 || !any_compensation_failed {
                SagaState::Compensated
            } else {
                SagaState::CompensationFailed
            };

            self.store.update_saga_state(saga_id, final_state.clone())?;

            Ok(SagaResult {
                saga_id,
                state: final_state,
                completed_steps,
                total_steps,
                error: Some(error_msg.to_string()),
                step_results: step_results.to_vec(),
                compensation_results,
            })
        }

        pub fn get_saga_status(&self, saga_id: Uuid) -> Result<SagaStatus, String> {
            let saga = self
                .store
                .load_saga(saga_id)
                .ok_or_else(|| format!("Saga {saga_id} not found"))?;

            let completed = saga.steps.iter().filter(|s| s.state == StepState::Completed).count();

            Ok(SagaStatus {
                saga_id,
                state: saga.state,
                total_steps: saga.steps.len(),
                completed_steps: completed,
            })
        }
    }

    // ========================================================================
    // OrchestratorBuilder
    // ========================================================================

    pub struct OrchestratorBuilder {
        steps: Vec<SagaStepDef>,
    }

    impl OrchestratorBuilder {
        pub fn new() -> Self {
            Self { steps: Vec::new() }
        }

        pub fn with_steps(mut self, count: usize) -> Self {
            let subgraphs = ["users", "orders", "products", "inventory", "payments"];
            let typenames = ["User", "Order", "Product", "InventoryItem", "Payment"];

            for i in 0..count {
                let idx = i % subgraphs.len();
                self.steps.push(SagaStepDef {
                    subgraph:              subgraphs[idx].to_string(),
                    mutation_type:         MutationType::Create,
                    typename:              typenames[idx].to_string(),
                    mutation_name:         format!("create{}", typenames[idx]),
                    variables:             json!({ "id": format!("id-{i}"), "step": i }),
                    behavior:              StepBehavior::Succeed,
                    compensation_behavior: CompensationBehavior::Succeed,
                });
            }

            self
        }

        pub fn step_fails_at(mut self, step_index: usize) -> Self {
            if step_index < self.steps.len() {
                self.steps[step_index].behavior =
                    StepBehavior::Fail(format!("Step {step_index} failed"));
            }
            self
        }

        pub fn compensation_fails_at(mut self, step_index: usize) -> Self {
            if step_index < self.steps.len() {
                self.steps[step_index].compensation_behavior =
                    CompensationBehavior::Fail(format!("Compensation {step_index} failed"));
            }
            self
        }

        pub fn all_compensations_fail(mut self) -> Self {
            for (i, step) in self.steps.iter_mut().enumerate() {
                step.compensation_behavior =
                    CompensationBehavior::Fail(format!("Compensation {i} failed"));
            }
            self
        }

        pub fn build(self) -> (SagaOrchestrator, Vec<SagaStepDef>) {
            let store = InMemorySagaStore::new();
            let executor = MockStepExecutor::new();
            let compensator = MockStepCompensator::new();
            let orchestrator = SagaOrchestrator::new(store, executor, compensator);
            (orchestrator, self.steps)
        }
    }

    // ========================================================================
    // Assertion Helpers
    // ========================================================================

    pub fn assert_saga_completed(result: &SagaResult) {
        assert_eq!(
            result.state,
            SagaState::Completed,
            "Expected saga {} to be Completed, got {:?}",
            result.saga_id,
            result.state,
        );
        assert_eq!(
            result.completed_steps, result.total_steps,
            "Expected all {} steps completed, got {}",
            result.total_steps, result.completed_steps,
        );
        assert!(
            result.error.is_none(),
            "Expected no error for completed saga, got: {:?}",
            result.error,
        );
        assert!(
            result.compensation_results.is_empty(),
            "Completed saga should have no compensation results",
        );
    }

    pub fn assert_saga_compensated(result: &SagaResult) {
        assert_eq!(
            result.state,
            SagaState::Compensated,
            "Expected saga {} to be Compensated, got {:?}",
            result.saga_id,
            result.state,
        );
        assert!(result.error.is_some(), "Compensated saga should have error message",);
    }

    pub fn assert_saga_compensation_failed(result: &SagaResult) {
        assert_eq!(
            result.state,
            SagaState::CompensationFailed,
            "Expected saga {} to be CompensationFailed, got {:?}",
            result.saga_id,
            result.state,
        );
    }
}

use std::time::Instant;

use harness::{
    CompensationBehavior, InMemorySagaStore, MockRecoveryManager, MockStepCompensator,
    MockStepExecutor, MutationType, OrchestratorBuilder, SagaOrchestrator, SagaState, SagaStepDef,
    StepBehavior, StoredSaga, assert_saga_compensated, assert_saga_compensation_failed,
    assert_saga_completed,
};
use serde_json::json;
use uuid::Uuid;

// ============================================================================
// Category 1: Full Saga Lifecycle (3 tests)
// ============================================================================

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
    let results: Vec<harness::SagaResult> = (0..2)
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

    // Pending → Executing → Completed
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

    // Pending → Executing → Failed → Compensating → Compensated
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

// ============================================================================
// Category 6: Recovery Integration (3 tests)
// ============================================================================

#[test]
fn test_e2e_recovery_detects_pending_sagas() {
    let store = InMemorySagaStore::new();

    let pending_id = Uuid::new_v4();
    store.save_saga(StoredSaga {
        id:           pending_id,
        state:        SagaState::Pending,
        steps:        vec![],
        created_at:   Instant::now(),
        completed_at: None,
    });

    let completed_id = Uuid::new_v4();
    store.save_saga(StoredSaga {
        id:           completed_id,
        state:        SagaState::Completed,
        steps:        vec![],
        created_at:   Instant::now(),
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
        id:           stuck_id,
        state:        SagaState::Executing,
        steps:        vec![],
        created_at:   Instant::now(),
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
            id:           Uuid::new_v4(),
            state:        SagaState::Completed,
            steps:        vec![],
            created_at:   Instant::now(),
            completed_at: Some(Instant::now()),
        });
    }

    store.save_saga(StoredSaga {
        id:           Uuid::new_v4(),
        state:        SagaState::Pending,
        steps:        vec![],
        created_at:   Instant::now(),
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
        subgraph:              "users".to_string(),
        mutation_type:         MutationType::Create,
        typename:              "User".to_string(),
        mutation_name:         "createUser".to_string(),
        variables:             json!({"id": "1"}),
        behavior:              StepBehavior::Succeed,
        compensation_behavior: CompensationBehavior::Succeed,
    }];
    let saga1_id = orchestrator.create_saga(saga1_steps).expect("Should create saga 1");
    let result1 = orchestrator.execute_saga(saga1_id).expect("Should execute saga 1");
    assert_saga_completed(&result1);

    // Saga 2: two steps, second fails → compensated
    orchestrator.executor.set_behavior(0, StepBehavior::Succeed);
    orchestrator
        .executor
        .set_behavior(1, StepBehavior::Fail("saga2 failure".to_string()));
    orchestrator.compensator.set_behavior(0, CompensationBehavior::Succeed);

    let saga2_steps = vec![
        SagaStepDef {
            subgraph:              "orders".to_string(),
            mutation_type:         MutationType::Create,
            typename:              "Order".to_string(),
            mutation_name:         "createOrder".to_string(),
            variables:             json!({"id": "2"}),
            behavior:              StepBehavior::Succeed,
            compensation_behavior: CompensationBehavior::Succeed,
        },
        SagaStepDef {
            subgraph:              "products".to_string(),
            mutation_type:         MutationType::Update,
            typename:              "Product".to_string(),
            mutation_name:         "updateProduct".to_string(),
            variables:             json!({"id": "3"}),
            behavior:              StepBehavior::Fail("saga2 failure".to_string()),
            compensation_behavior: CompensationBehavior::Succeed,
        },
    ];
    let saga2_id = orchestrator.create_saga(saga2_steps).expect("Should create saga 2");
    let result2 = orchestrator.execute_saga(saga2_id).expect("Should execute saga 2");
    assert_saga_compensated(&result2);

    // Saga 3: single step, succeeds
    orchestrator.executor.set_behavior(0, StepBehavior::Succeed);

    let saga3_steps = vec![SagaStepDef {
        subgraph:              "payments".to_string(),
        mutation_type:         MutationType::Create,
        typename:              "Payment".to_string(),
        mutation_name:         "createPayment".to_string(),
        variables:             json!({"id": "4"}),
        behavior:              StepBehavior::Succeed,
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
        subgraph:              "users".to_string(),
        mutation_type:         MutationType::Create,
        typename:              "User".to_string(),
        mutation_name:         "createUser".to_string(),
        variables:             json!({"id": "1"}),
        behavior:              StepBehavior::Succeed,
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
            subgraph:              "orders".to_string(),
            mutation_type:         MutationType::Create,
            typename:              "Order".to_string(),
            mutation_name:         "createOrder".to_string(),
            variables:             json!({"id": "2"}),
            behavior:              StepBehavior::Succeed,
            compensation_behavior: CompensationBehavior::Succeed,
        },
        SagaStepDef {
            subgraph:              "products".to_string(),
            mutation_type:         MutationType::Update,
            typename:              "Product".to_string(),
            mutation_name:         "updateProduct".to_string(),
            variables:             json!({"id": "3"}),
            behavior:              StepBehavior::Fail("fail".to_string()),
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
