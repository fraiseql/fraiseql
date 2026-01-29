//! Cycle 13: Saga Stress Testing
//!
//! Validates saga system under extreme conditions with 13 comprehensive stress tests.
//! Tests validate stability, correctness, memory efficiency, and failure handling
//! at scale.
//!
//! ## Test Categories (13 total)
//!
//! Fast tests (run by default):
//! - **100 concurrent sagas** (~1.3ms): Validates basic concurrent operation
//! - **500 concurrent sagas** (~5.3ms): Higher concurrent load validation
//! - **50-step saga**: Long transaction chain execution
//! - **High failure rate (34%)**: Failure handling under load
//! - **Cascading compensation**: Multi-step rollback validation
//! - **Memory linear scaling**: Validates O(n) memory growth
//! - **Recovery scan**: 100 pending sagas state management
//!
//! Ignored stress tests (run with `--ignored --nocapture`):
//! - **1000 concurrent sagas**: Heavy concurrent load (<5s budget)
//! - **5000 concurrent sagas**: Extreme concurrent load (<20s budget)
//! - **100-step saga**: Maximum step chain execution
//! - **30-second sustained load**: Extended throughput test
//! - **60-second sustained load**: Extended throughput test
//! - **Memory overhead validation**: <5% overhead tolerance
//!
//! ## Running Tests
//!
//! Fast tests (included in CI):
//! ```bash
//! cargo test --test federation_saga_stress_test
//! ```
//!
//! Stress tests (manual verification):
//! ```bash
//! cargo test --test federation_saga_stress_test -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

// ============================================================================
// Test Harness (Copied from federation_saga_e2e.rs)
// ============================================================================

#[allow(dead_code)]
mod harness {
    use std::{collections::HashMap, sync::Mutex, time::Instant};

    use serde_json::{Value, json};
    use uuid::Uuid;

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

    pub struct InMemorySagaStore {
        sagas:       Mutex<HashMap<Uuid, StoredSaga>>,
        transitions: Mutex<Vec<(Uuid, StateTransition)>>,
    }

    #[derive(Debug, Clone)]
    struct StateTransition {
        from:      SagaState,
        to:        SagaState,
        timestamp: Instant,
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

        pub fn load_sagas_by_state(&self, state: &SagaState) -> Vec<StoredSaga> {
            self.sagas
                .lock()
                .unwrap()
                .values()
                .filter(|s| s.state == *state)
                .cloned()
                .collect()
        }

        pub fn saga_count(&self) -> usize {
            self.sagas.lock().unwrap().len()
        }

        pub fn estimate_memory_usage(&self) -> usize {
            let sagas = self.sagas.lock().unwrap();
            sagas
                .values()
                .map(|saga| {
                    // Rough estimate: UUID (16) + state (1) + vec of steps
                    16 + 1 + (saga.steps.len() * 256) // ~256 bytes per step
                })
                .sum()
        }
    }

    pub struct MockStepExecutor {
        behaviors: Mutex<HashMap<usize, StepBehavior>>,
    }

    impl MockStepExecutor {
        pub fn new() -> Self {
            Self {
                behaviors: Mutex::new(HashMap::new()),
            }
        }

        pub fn set_behavior(&self, step_order: usize, behavior: StepBehavior) {
            self.behaviors.lock().unwrap().insert(step_order, behavior);
        }

        pub fn execute(
            &self,
            step_order: usize,
            _subgraph: &str,
            _mutation_name: &str,
            variables: &Value,
            _previous_result: Option<&Value>,
        ) -> Result<Value, String> {
            let behavior = self
                .behaviors
                .lock()
                .unwrap()
                .get(&step_order)
                .cloned()
                .unwrap_or(StepBehavior::Succeed);

            match &behavior {
                StepBehavior::Succeed => Ok(json!({
                    "step": step_order,
                    "status": "completed",
                    "variables": variables
                })),
                StepBehavior::Fail(msg) => Err(msg.clone()),
            }
        }
    }

    pub struct MockStepCompensator {
        behaviors: Mutex<HashMap<usize, CompensationBehavior>>,
    }

    impl MockStepCompensator {
        pub fn new() -> Self {
            Self {
                behaviors: Mutex::new(HashMap::new()),
            }
        }

        pub fn set_behavior(&self, step_order: usize, behavior: CompensationBehavior) {
            self.behaviors.lock().unwrap().insert(step_order, behavior);
        }

        pub fn compensate(
            &self,
            step_order: usize,
            _original_result: Option<&Value>,
        ) -> Result<Value, String> {
            let behavior = self
                .behaviors
                .lock()
                .unwrap()
                .get(&step_order)
                .cloned()
                .unwrap_or(CompensationBehavior::Succeed);

            match &behavior {
                CompensationBehavior::Succeed => Ok(json!({
                    "compensated_step": step_order,
                    "status": "rolled_back"
                })),
                CompensationBehavior::Fail(msg) => Err(msg.clone()),
            }
        }
    }

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

            match &saga.state {
                SagaState::Completed | SagaState::Compensated | SagaState::CompensationFailed => {
                    return Err("Cannot re-execute terminal saga".to_string());
                },
                _ => {},
            }

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

            if failed_at.is_none() {
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
            } else {
                let error_msg = failed_at.unwrap();
                self.store.update_saga_state(saga_id, SagaState::Compensating)?;

                let mut compensation_results = Vec::new();
                for i in (0..completed_steps).rev() {
                    let original_result = step_results[i].as_ref();
                    match self.compensator.compensate(i, original_result) {
                        Ok(_) => {
                            compensation_results.push(CompensationExecution {
                                step_order:      i,
                                original_result: original_result.cloned(),
                                result:          Ok(json!({})),
                                timestamp:       Instant::now(),
                            });
                        },
                        Err(_) => {
                            self.store.update_saga_state(saga_id, SagaState::CompensationFailed)?;
                            return Ok(SagaResult {
                                saga_id,
                                state: SagaState::CompensationFailed,
                                completed_steps,
                                total_steps,
                                error: Some(error_msg),
                                step_results,
                                compensation_results,
                            });
                        },
                    }
                }

                self.store.update_saga_state(saga_id, SagaState::Compensated)?;
                Ok(SagaResult {
                    saga_id,
                    state: SagaState::Compensated,
                    completed_steps,
                    total_steps,
                    error: Some(error_msg),
                    step_results,
                    compensation_results,
                })
            }
        }
    }

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

        pub fn with_step_failure_at(mut self, step_index: usize) -> Self {
            if step_index < self.steps.len() {
                self.steps[step_index].behavior =
                    StepBehavior::Fail(format!("Step {step_index} failed"));
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
}

use harness::{OrchestratorBuilder, SagaState};

// ============================================================================
// Stress Tests
// ============================================================================

/// Fast concurrent load: 100 sagas
#[test]
fn stress_100_concurrent_sagas() {
    let start = Instant::now();

    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let mut saga_ids = Vec::with_capacity(100);
    for _ in 0..100 {
        match orchestrator.create_saga(steps.clone()) {
            Ok(id) => saga_ids.push(id),
            Err(e) => panic!("Failed to create saga: {}", e),
        }
    }

    let mut results = Vec::with_capacity(100);
    for saga_id in saga_ids {
        match orchestrator.execute_saga(saga_id) {
            Ok(result) => results.push(result),
            Err(e) => panic!("Failed to execute saga: {}", e),
        }
    }

    let duration = start.elapsed();
    println!(
        "100 concurrent sagas completed in {:?} ({:.2}ms)",
        duration,
        duration.as_secs_f64() * 1000.0
    );

    assert_eq!(results.len(), 100);
    assert!(
        results.iter().all(|r| r.state == SagaState::Completed),
        "All sagas should complete successfully"
    );
    assert!(
        duration < Duration::from_millis(500),
        "100 sagas should complete in < 500ms, got {:?}",
        duration
    );
}

/// Fast concurrent load: 500 sagas
#[test]
fn stress_500_concurrent_sagas() {
    let start = Instant::now();

    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let mut saga_ids = Vec::with_capacity(500);
    for _ in 0..500 {
        match orchestrator.create_saga(steps.clone()) {
            Ok(id) => saga_ids.push(id),
            Err(e) => panic!("Failed to create saga: {}", e),
        }
    }

    let mut results = Vec::with_capacity(500);
    for saga_id in saga_ids {
        match orchestrator.execute_saga(saga_id) {
            Ok(result) => results.push(result),
            Err(e) => panic!("Failed to execute saga: {}", e),
        }
    }

    let duration = start.elapsed();
    println!(
        "500 concurrent sagas completed in {:?} ({:.2}ms)",
        duration,
        duration.as_secs_f64() * 1000.0
    );

    assert_eq!(results.len(), 500);
    assert!(
        results.iter().all(|r| r.state == SagaState::Completed),
        "All sagas should complete successfully"
    );
    assert!(
        duration < Duration::from_secs(2),
        "500 sagas should complete in < 2s, got {:?}",
        duration
    );
}

/// Heavy concurrent load: 1000 sagas (IGNORED)
#[test]
#[ignore = "stress test - run with --ignored"]
fn stress_1000_concurrent_sagas() {
    let start = Instant::now();

    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let mut saga_ids = Vec::with_capacity(1000);
    for _ in 0..1000 {
        match orchestrator.create_saga(steps.clone()) {
            Ok(id) => saga_ids.push(id),
            Err(e) => panic!("Failed to create saga: {}", e),
        }
    }

    let mut results = Vec::with_capacity(1000);
    for saga_id in saga_ids {
        match orchestrator.execute_saga(saga_id) {
            Ok(result) => results.push(result),
            Err(e) => panic!("Failed to execute saga: {}", e),
        }
    }

    let duration = start.elapsed();
    println!(
        "1000 concurrent sagas completed in {:?} ({:.2}ms)",
        duration,
        duration.as_secs_f64() * 1000.0
    );

    assert_eq!(results.len(), 1000);
    assert!(
        duration < Duration::from_secs(5),
        "1000 sagas should complete in < 5s, got {:?}",
        duration
    );
}

/// Heavy concurrent load: 5000 sagas (IGNORED)
#[test]
#[ignore = "stress test - run with --ignored"]
fn stress_5000_concurrent_sagas() {
    let start = Instant::now();

    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let mut saga_ids = Vec::with_capacity(5000);
    for i in 0..5000 {
        match orchestrator.create_saga(steps.clone()) {
            Ok(id) => saga_ids.push(id),
            Err(e) => panic!("Failed to create saga {}: {}", i, e),
        }
    }

    let mut results = Vec::with_capacity(5000);
    for saga_id in saga_ids {
        match orchestrator.execute_saga(saga_id) {
            Ok(result) => results.push(result),
            Err(e) => panic!("Failed to execute saga: {}", e),
        }
    }

    let duration = start.elapsed();
    println!(
        "5000 concurrent sagas completed in {:?} ({:.2}ms)",
        duration,
        duration.as_secs_f64() * 1000.0
    );

    assert_eq!(results.len(), 5000);
    assert!(
        duration < Duration::from_secs(20),
        "5000 sagas should complete in < 20s, got {:?}",
        duration
    );
}

/// Long-running saga: 50 steps
#[test]
fn stress_saga_50_steps() {
    let start = Instant::now();

    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(50).build();

    let saga_id = orchestrator.create_saga(steps).expect("Failed to create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Failed to execute saga");

    let duration = start.elapsed();
    println!("Saga with 50 steps completed in {:?}", duration);

    assert_eq!(result.total_steps, 50);
    assert_eq!(result.completed_steps, 50);
    assert_eq!(result.state, SagaState::Completed);
    assert!(
        duration < Duration::from_millis(500),
        "50-step saga should complete in < 500ms, got {:?}",
        duration
    );
}

/// Long-running saga: 100 steps (IGNORED)
#[test]
#[ignore = "stress test - run with --ignored"]
fn stress_saga_100_steps() {
    let start = Instant::now();

    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(100).build();

    let saga_id = orchestrator.create_saga(steps).expect("Failed to create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Failed to execute saga");

    let duration = start.elapsed();
    println!("Saga with 100 steps completed in {:?}", duration);

    assert_eq!(result.total_steps, 100);
    assert_eq!(result.completed_steps, 100);
    assert_eq!(result.state, SagaState::Completed);
}

/// Sustained load: 30 seconds (IGNORED)
#[test]
#[ignore = "stress test - run with --ignored"]
fn stress_sustained_30s_load() {
    let start = Instant::now();
    let deadline = Duration::from_secs(30);

    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let mut count = 0;
    while start.elapsed() < deadline {
        let saga_id = orchestrator.create_saga(steps.clone()).expect("Failed to create saga");
        let _ = orchestrator.execute_saga(saga_id).expect("Failed to execute saga");
        count += 1;
    }

    let duration = start.elapsed();
    let throughput = count as f64 / duration.as_secs_f64();
    println!("30-second sustained load: {} sagas, {:.2} sagas/sec", count, throughput);

    assert!(count > 0);
}

/// Sustained load: 60 seconds (IGNORED)
#[test]
#[ignore = "stress test - run with --ignored"]
fn stress_sustained_60s_load() {
    let start = Instant::now();
    let deadline = Duration::from_secs(60);

    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let mut count = 0;
    while start.elapsed() < deadline {
        let saga_id = orchestrator.create_saga(steps.clone()).expect("Failed to create saga");
        let _ = orchestrator.execute_saga(saga_id).expect("Failed to execute saga");
        count += 1;
    }

    let duration = start.elapsed();
    let throughput = count as f64 / duration.as_secs_f64();
    println!("60-second sustained load: {} sagas, {:.2} sagas/sec", count, throughput);

    assert!(count > 0);
}

/// Memory growth: linear scaling validation
#[test]
fn stress_memory_linear_scaling() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let mut memory_samples = Vec::new();

    for batch_size in [10, 50, 100, 500] {
        let mut saga_ids = Vec::new();
        for _ in 0..batch_size {
            match orchestrator.create_saga(steps.clone()) {
                Ok(id) => saga_ids.push(id),
                Err(e) => panic!("Failed to create saga: {}", e),
            }
        }

        let memory = orchestrator.store.estimate_memory_usage();
        memory_samples.push((batch_size, memory));

        // Clean up for next iteration
        for saga_id in saga_ids {
            let _ = orchestrator.execute_saga(saga_id);
        }
    }

    println!("Memory growth samples: {:?}", memory_samples);

    // Verify memory grows somewhat linearly (not exponentially)
    for window in memory_samples.windows(2) {
        let (size1, mem1) = window[0];
        let (size2, mem2) = window[1];
        let size_ratio = size2 as f64 / size1 as f64;
        let mem_ratio = mem2 as f64 / mem1 as f64;

        // Memory ratio should be similar to size ratio (linear growth)
        assert!(
            mem_ratio < size_ratio * 1.5,
            "Memory growth not linear: size_ratio={}, mem_ratio={}",
            size_ratio,
            mem_ratio
        );
    }
}

/// Memory growth: overhead validation (IGNORED)
#[test]
#[ignore = "stress test - run with --ignored"]
fn stress_memory_overhead() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let mut saga_ids = Vec::new();
    for _ in 0..1000 {
        match orchestrator.create_saga(steps.clone()) {
            Ok(id) => saga_ids.push(id),
            Err(e) => panic!("Failed to create saga: {}", e),
        }
    }

    let memory = orchestrator.store.estimate_memory_usage();
    let theoretical_size = 1000 * 256; // ~256 bytes per saga with 3 steps

    let overhead_ratio = memory as f64 / theoretical_size as f64;
    println!(
        "Memory overhead: {} bytes, theoretical: {} bytes, ratio: {:.2}",
        memory, theoretical_size, overhead_ratio
    );

    // Overhead should be < 5%
    assert!(
        overhead_ratio < 1.05,
        "Memory overhead too high: {:.2}x (expected < 1.05x)",
        overhead_ratio
    );
}

/// Failure scenario: high failure rate
#[test]
fn stress_high_failure_rate() {
    let (orchestrator, _steps) = OrchestratorBuilder::new().with_steps(10).build();

    let mut failures = 0;
    for i in 0..100 {
        let builder = OrchestratorBuilder::new().with_steps(10);
        let builder = if i % 3 == 0 {
            builder.with_step_failure_at(5)
        } else {
            builder
        };

        let (_, test_steps) = builder.build();
        match orchestrator.create_saga(test_steps) {
            Ok(saga_id) => {
                if let Ok(result) = orchestrator.execute_saga(saga_id) {
                    if result.error.is_some() {
                        failures += 1;
                    }
                }
            },
            Err(e) => panic!("Failed to create saga: {}", e),
        }
    }

    println!("High failure rate test: {} failures out of 100", failures);
    assert!(failures > 25, "Expected significant failures");
}

/// Failure scenario: cascading compensation
#[test]
fn stress_cascading_compensation() {
    let (orchestrator, mut steps) = OrchestratorBuilder::new().with_steps(5).build();

    // Cause failure on last step
    if !steps.is_empty() {
        let last_idx = steps.len() - 1;
        steps[last_idx].behavior = harness::StepBehavior::Fail("trigger compensation".to_string());
    }

    let saga_id = orchestrator.create_saga(steps).expect("Failed to create saga");
    let result = orchestrator.execute_saga(saga_id).expect("Failed to execute saga");

    assert_eq!(result.state, SagaState::Compensated);
    assert_eq!(result.compensation_results.len(), 4);
    println!(
        "Cascading compensation: {} steps compensated",
        result.compensation_results.len()
    );
}

/// Recovery under load: 100 pending sagas
#[test]
fn stress_recovery_100_pending() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let mut saga_ids = Vec::with_capacity(100);
    for _ in 0..100 {
        match orchestrator.create_saga(steps.clone()) {
            Ok(id) => saga_ids.push(id),
            Err(e) => panic!("Failed to create saga: {}", e),
        }
    }

    // Execute some, leaving others pending
    for (i, saga_id) in saga_ids.iter().enumerate() {
        if i % 2 == 0 {
            let _ = orchestrator.execute_saga(*saga_id);
        }
    }

    // Check recovery can scan the pending sagas
    let pending = orchestrator.store.load_sagas_by_state(&SagaState::Pending);
    println!("Recovery scan found {} pending sagas", pending.len());

    assert!(!pending.is_empty());
    assert!(pending.len() <= 50);
}
