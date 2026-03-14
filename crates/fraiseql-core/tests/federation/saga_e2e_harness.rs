//! In-memory saga test harness for E2E integration tests.

#![allow(dead_code)]
#![allow(clippy::needless_pass_by_value)] // Reason: test harness methods mirror production API signatures
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::needless_collect)] // Reason: intermediate collect needed to spawn all tasks before joining
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
        let saga = sagas.get_mut(&saga_id).ok_or_else(|| format!("Saga {saga_id} not found"))?;
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
        let saga = sagas.get_mut(&saga_id).ok_or_else(|| format!("Saga {saga_id} not found"))?;
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
    pub const fn new(store: &'a InMemorySagaStore) -> Self {
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
    pub const fn new(
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
    pub const fn new() -> Self {
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
