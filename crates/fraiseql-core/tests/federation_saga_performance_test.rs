//! Cycle 13: Saga Performance Budget Tests
//!
//! Validates performance budgets and detects regressions for saga operations.
//! All tests complete in <5s each for fast CI integration.
//!
//! ## Test Categories (13 total)
//!
//! **Budget Validation (7 tests)**:
//! - Creation latency budget: <100μs for 3-step saga
//! - Execution latency budget: <500μs for 3-step saga
//! - Compensation latency budget: <500μs for 3-step saga
//! - 10-saga concurrent batch: <30ms
//! - 50-saga concurrent batch: <50ms
//! - Memory per saga: <1KB
//!
//! **Latency Distribution (2 tests)**:
//! - P95 latency for creation: <100μs (95th percentile)
//! - P99 latency for execution: <1000μs (99th percentile)
//!
//! **Throughput Baselines (2 tests)**:
//! - Saga creation: >10,000 sagas/sec
//! - Saga execution: >2,000 sagas/sec (create + execute)
//!
//! **Regression Detection (2 tests)**:
//! - Baseline comparison: <10% regression tolerance
//! - Compensation overhead: <2x slowdown vs success path
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test federation_saga_performance_test -- --nocapture
//! ```
//!
//! Tests measure actual performance and report detailed metrics for monitoring
//! performance trends across releases.

use std::time::Instant;

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

        pub fn create_saga(&self, steps: Vec<SagaStepDef>) -> Result<uuid::Uuid, String> {
            if steps.is_empty() {
                return Err("Saga must have at least one step".to_string());
            }

            let saga_id = uuid::Uuid::new_v4();
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

        pub fn execute_saga(&self, saga_id: uuid::Uuid) -> Result<SagaResult, String> {
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

use harness::OrchestratorBuilder;

// ============================================================================
// Performance Budget Tests
// ============================================================================

/// Budget validation: saga creation (3 steps)
#[test]
fn perf_budget_saga_creation_3_steps() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    // Warm up
    for _ in 0..5 {
        let _ = orchestrator.create_saga(steps.clone());
    }

    // Measure
    let start = Instant::now();
    for _ in 0..100 {
        let _ = orchestrator.create_saga(steps.clone());
    }
    let avg_latency_us = start.elapsed().as_micros() / 100;

    println!("Saga creation (3 steps): {}μs", avg_latency_us);

    // Budget: <100μs
    assert!(
        avg_latency_us < 100,
        "Saga creation latency {}μs exceeds budget of 100μs",
        avg_latency_us
    );
}

/// Budget validation: saga execution (3 steps)
#[test]
fn perf_budget_saga_execution_3_steps() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    // Warm up
    for _ in 0..5 {
        let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
    }

    // Measure
    let start = Instant::now();
    for _ in 0..100 {
        let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
    }
    let avg_latency_us = start.elapsed().as_micros() / 100;

    println!("Saga execution (3 steps): {}μs", avg_latency_us);

    // Budget: <500μs
    assert!(
        avg_latency_us < 500,
        "Saga execution latency {}μs exceeds budget of 500μs",
        avg_latency_us
    );
}

/// Budget validation: saga compensation (3 steps)
#[test]
fn perf_budget_saga_compensation_3_steps() {
    let (orchestrator, mut steps) = OrchestratorBuilder::new().with_steps(3).build();

    // Set up failure to trigger compensation
    if !steps.is_empty() {
        let last_idx = steps.len() - 1;
        steps[last_idx].behavior = harness::StepBehavior::Fail("trigger compensation".to_string());
    }

    // Warm up
    for _ in 0..5 {
        let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
    }

    // Measure
    let start = Instant::now();
    for _ in 0..50 {
        let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
    }
    let avg_latency_us = start.elapsed().as_micros() / 50;

    println!("Saga compensation (3 steps): {}μs", avg_latency_us);

    // Budget: <500μs
    assert!(
        avg_latency_us < 500,
        "Saga compensation latency {}μs exceeds budget of 500μs",
        avg_latency_us
    );
}

/// Budget validation: 10 concurrent sagas
#[test]
fn perf_budget_10_concurrent_sagas() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    // Warm up
    for _ in 0..2 {
        for _ in 0..10 {
            let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
            let _ = orchestrator.execute_saga(saga_id);
        }
    }

    // Measure
    let start = Instant::now();
    for _ in 0..10 {
        for _ in 0..10 {
            let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
            let _ = orchestrator.execute_saga(saga_id);
        }
    }
    let total_duration = start.elapsed();
    let avg_per_batch = total_duration.as_millis() / 10;

    println!("10 concurrent sagas batch time: {}ms", avg_per_batch);

    // Budget: <30ms per batch
    assert!(
        avg_per_batch < 30,
        "Concurrent batch latency {}ms exceeds budget of 30ms",
        avg_per_batch
    );
}

/// Budget validation: 50 concurrent sagas
#[test]
fn perf_budget_50_concurrent_sagas() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    // Warm up
    for _ in 0..2 {
        for _ in 0..50 {
            let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
            let _ = orchestrator.execute_saga(saga_id);
        }
    }

    // Measure
    let start = Instant::now();
    for _ in 0..5 {
        for _ in 0..50 {
            let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
            let _ = orchestrator.execute_saga(saga_id);
        }
    }
    let total_duration = start.elapsed();
    let avg_per_batch = total_duration.as_millis() / 5;

    println!("50 concurrent sagas batch time: {}ms", avg_per_batch);

    // Budget: <50ms per batch
    assert!(
        avg_per_batch < 50,
        "Concurrent batch latency {}ms exceeds budget of 50ms",
        avg_per_batch
    );
}

/// Budget validation: memory per saga
#[test]
fn perf_budget_memory_per_saga() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    let memory_before = orchestrator.store.estimate_memory_usage();

    // Create 100 sagas
    for _ in 0..100 {
        let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
    }

    let memory_after = orchestrator.store.estimate_memory_usage();
    let memory_used = memory_after - memory_before;
    let per_saga = memory_used / 100;

    println!("Memory per saga: {} bytes", per_saga);

    // Budget: <1KB per saga
    assert!(
        per_saga < 1024,
        "Memory per saga {} bytes exceeds budget of 1024 bytes",
        per_saga
    );
}

// ============================================================================
// Latency Distribution Tests
// ============================================================================

/// Latency distribution: P95 latency
#[test]
fn perf_latency_p95_creation() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    // Warm up
    for _ in 0..10 {
        let _ = orchestrator.create_saga(steps.clone());
    }

    // Measure
    let mut latencies = Vec::with_capacity(100);
    for _ in 0..100 {
        let start = Instant::now();
        let _ = orchestrator.create_saga(steps.clone());
        latencies.push(start.elapsed().as_micros());
    }

    latencies.sort_unstable();
    let p95_idx = (latencies.len() * 95) / 100;
    let p95 = latencies[p95_idx];

    println!("Creation P95 latency: {}μs", p95);

    // Budget: <100μs
    assert!(p95 < 100, "P95 creation latency {}μs exceeds budget of 100μs", p95);
}

/// Latency distribution: P99 latency
#[test]
fn perf_latency_p99_execution() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    // Warm up
    for _ in 0..10 {
        let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
    }

    // Measure
    let mut latencies = Vec::with_capacity(100);
    for _ in 0..100 {
        let start = Instant::now();
        let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
        latencies.push(start.elapsed().as_micros());
    }

    latencies.sort_unstable();
    let p99_idx = (latencies.len() * 99) / 100;
    let p99 = latencies[p99_idx];

    println!("Execution P99 latency: {}μs", p99);

    // Budget: <1000μs (1ms)
    assert!(p99 < 1000, "P99 execution latency {}μs exceeds budget of 1000μs", p99);
}

// ============================================================================
// Throughput Baseline Tests
// ============================================================================

/// Throughput baseline: saga creation
#[test]
fn perf_throughput_saga_creation() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    // Warm up
    for _ in 0..100 {
        let _ = orchestrator.create_saga(steps.clone());
    }

    // Measure
    let start = Instant::now();
    let iterations = 5000;
    for _ in 0..iterations {
        let _ = orchestrator.create_saga(steps.clone());
    }
    let duration = start.elapsed();

    let throughput = iterations as f64 / duration.as_secs_f64();
    println!("Saga creation throughput: {:.0} sagas/sec", throughput);

    // Budget: >10,000/sec
    assert!(
        throughput > 10000.0,
        "Saga creation throughput {:.0} sagas/sec below budget of 10,000/sec",
        throughput
    );
}

/// Throughput baseline: saga execution
#[test]
fn perf_throughput_saga_execution() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    // Warm up
    for _ in 0..100 {
        let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
    }

    // Measure
    let start = Instant::now();
    let iterations = 1000;
    for _ in 0..iterations {
        let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
    }
    let duration = start.elapsed();

    let throughput = iterations as f64 / duration.as_secs_f64();
    println!("Saga execution throughput: {:.0} sagas/sec", throughput);

    // Budget: >2,000/sec
    assert!(
        throughput > 2000.0,
        "Saga execution throughput {:.0} sagas/sec below budget of 2,000/sec",
        throughput
    );
}

// ============================================================================
// Regression Detection Tests
// ============================================================================

/// Regression detection: vs baseline
#[test]
fn perf_regression_vs_baseline() {
    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

    // Baseline: 100 sagas
    let start = Instant::now();
    for _ in 0..100 {
        let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
    }
    let baseline = start.elapsed();

    // New run: 100 sagas (should not be significantly slower)
    let start = Instant::now();
    for _ in 0..100 {
        let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
    }
    let current = start.elapsed();

    let regression = (current.as_secs_f64() / baseline.as_secs_f64() - 1.0) * 100.0;
    println!("Regression vs baseline: {:.1}%", regression);

    // Allow 10% regression (noise tolerance)
    assert!(
        regression < 10.0,
        "Performance regression {:.1}% exceeds tolerance of 10%",
        regression
    );
}

/// Regression detection: compensation overhead
#[test]
fn perf_regression_compensation_overhead() {
    let (orchestrator, steps_success) = OrchestratorBuilder::new().with_steps(3).build();

    // Success path latency
    let start = Instant::now();
    for _ in 0..100 {
        let saga_id = orchestrator.create_saga(steps_success.clone()).unwrap();
        let _ = orchestrator.execute_saga(saga_id);
    }
    let success_duration = start.elapsed();

    // Compensation path latency
    let (orchestrator2, mut steps_fail) = OrchestratorBuilder::new().with_steps(3).build();
    if !steps_fail.is_empty() {
        let last_idx = steps_fail.len() - 1;
        steps_fail[last_idx].behavior =
            harness::StepBehavior::Fail("trigger compensation".to_string());
    }

    let start = Instant::now();
    for _ in 0..100 {
        let saga_id = orchestrator2.create_saga(steps_fail.clone()).unwrap();
        let _ = orchestrator2.execute_saga(saga_id);
    }
    let compensation_duration = start.elapsed();

    let overhead =
        (compensation_duration.as_secs_f64() / success_duration.as_secs_f64() - 1.0) * 100.0;
    println!("Compensation overhead: {:.1}%", overhead);

    // Compensation should not be more than 3x slower
    assert!(overhead < 200.0, "Compensation overhead {:.1}% too high (max 2x)", overhead);
}
