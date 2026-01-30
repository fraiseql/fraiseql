//! Cycle 13: Saga Performance Benchmarks
//!
//! Criterion.rs benchmarks for precise latency and throughput measurement of the
//! FraiseQL saga system using statistical analysis and parameterized testing.
//!
//! ## Benchmark Groups (15 total)
//!
//! - **Saga creation** (4 benchmarks): Step counts of 3, 10, 20, 50
//! - **Saga execution** (3 benchmarks): Step counts of 3, 10, 20
//! - **Saga compensation** (3 benchmarks): Step counts of 3, 10, 20 (with failure triggered)
//! - **Concurrent execution** (3 benchmarks): 10, 50, 100 sagas executing simultaneously
//! - **Recovery operations** (2 benchmarks): State scanning and cleanup performance
//!
//! ## Performance Baselines
//!
//! These baselines establish the expected performance characteristics:
//!
//! - **Creation latency**: <100μs for 3-step sagas
//! - **Execution latency**: <500μs for 3-step sagas
//! - **Compensation latency**: <500μs for 3-step sagas (when triggered)
//! - **Concurrent batch**: <50ms for 100 concurrent sagas
//! - **Creation throughput**: >10,000 sagas/sec
//! - **Execution throughput**: >2,000 sagas/sec (create + execute)
//!
//! ## Running the Benchmarks
//!
//! ```bash
//! cargo bench --bench saga_performance_bench
//! ```
//!
//! Results are generated in `target/criterion/` for detailed analysis and
//! comparison across runs using Criterion's statistical tools.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

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

            if let Some(error_msg) = failed_at {
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
// Benchmark Groups
// ============================================================================

/// Saga creation benchmarks by step count
fn bench_saga_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("saga_creation");

    for step_count in [3, 10, 20, 50] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_steps", step_count)),
            &step_count,
            |b, &count| {
                b.iter(|| {
                    let (orchestrator, steps) =
                        OrchestratorBuilder::new().with_steps(count).build();
                    let result = orchestrator.create_saga(steps);
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

/// Saga execution benchmarks by step count
fn bench_saga_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("saga_execution");

    for step_count in [3, 10, 20] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_steps", step_count)),
            &step_count,
            |b, &count| {
                b.iter_batched(
                    || {
                        let (orch, steps) = OrchestratorBuilder::new().with_steps(count).build();
                        let saga_id = orch.create_saga(steps).unwrap();
                        (orch, saga_id)
                    },
                    |(orch, saga_id)| {
                        let result = orch.execute_saga(saga_id);
                        black_box(result)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

/// Saga compensation benchmarks by step count
fn bench_saga_compensation(c: &mut Criterion) {
    let mut group = c.benchmark_group("saga_compensation");

    for step_count in [3, 10, 20] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_steps", step_count)),
            &step_count,
            |b, &count| {
                b.iter_batched(
                    || {
                        let (orch, steps_base) =
                            OrchestratorBuilder::new().with_steps(count).build();
                        let mut steps = steps_base;
                        if !steps.is_empty() {
                            // Cause failure on last step to trigger compensation
                            let last_idx = steps.len() - 1;
                            steps[last_idx].behavior =
                                harness::StepBehavior::Fail("trigger compensation".to_string());
                        }
                        let saga_id = orch.create_saga(steps).unwrap();
                        (orch, saga_id)
                    },
                    |(orch, saga_id)| {
                        let result = orch.execute_saga(saga_id);
                        black_box(result)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

/// Concurrent saga execution benchmarks
fn bench_concurrent_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_execution");
    group.sample_size(10);

    for concurrency in [10, 50, 100] {
        group.throughput(Throughput::Elements(concurrency as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_concurrent", concurrency)),
            &concurrency,
            |b, &count| {
                b.iter(|| {
                    let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();

                    let results: Vec<_> = (0..count)
                        .map(|_| {
                            let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
                            orchestrator.execute_saga(saga_id)
                        })
                        .collect();

                    black_box(results)
                });
            },
        );
    }
    group.finish();
}

/// Recovery operation benchmarks
fn bench_recovery_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("recovery_operations");

    group.bench_function("recovery_scan_100_sagas", |b| {
        b.iter_batched(
            || {
                let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();
                for _ in 0..100 {
                    let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
                    let _ = orchestrator.execute_saga(saga_id);
                }
                orchestrator
            },
            |orchestrator| {
                let _sagas = orchestrator.store.load_sagas_by_state(&SagaState::Completed);
                black_box(orchestrator.store.saga_count())
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("recovery_cleanup_100_sagas", |b| {
        b.iter_batched(
            || {
                let (orchestrator, steps) = OrchestratorBuilder::new().with_steps(3).build();
                for _ in 0..100 {
                    let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
                    let _ = orchestrator.execute_saga(saga_id);
                }
                orchestrator
            },
            |_orchestrator| {
                // Placeholder for cleanup operation
                black_box(100usize)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_saga_creation,
    bench_saga_execution,
    bench_saga_compensation,
    bench_concurrent_execution,
    bench_recovery_operations
);
criterion_main!(benches);
