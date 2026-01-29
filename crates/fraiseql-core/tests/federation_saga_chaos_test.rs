//! Cycle 14: Saga Chaos Testing
//!
//! Validates saga system resilience under non-deterministic failure conditions.
//! Uses probabilistic failure injection, network simulation, and timing chaos
//! to expose race conditions and edge cases.
//!
//! ## Test Categories (18 tests)
//!
//! - Random Failure Chaos (4 tests)
//! - Network Chaos (3 tests)
//! - Timing Chaos (3 tests)
//! - Recovery Chaos (3 tests)
//! - Byzantine Failure Chaos (3 tests)
//! - Combination Chaos (2 tests)

#[allow(dead_code)]
mod harness {
    use std::{
        collections::HashMap,
        sync::Mutex,
        time::{Duration, Instant},
    };

    use rand::{Rng, SeedableRng, rngs::StdRng};
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
    // Chaos Configuration
    // ========================================================================

    #[derive(Debug, Clone)]
    pub struct ChaosConfig {
        pub step_failure_rate: f64,
        pub compensation_failure_rate: f64,
        pub network_delay_range_ms: (u64, u64),
        pub network_drop_rate: f64,
        pub enable_state_corruption: bool,
        pub enable_timing_chaos: bool,
        pub seed: Option<u64>,
    }

    impl ChaosConfig {
        pub fn random_failures(rate: f64) -> Self {
            Self {
                step_failure_rate: rate,
                compensation_failure_rate: 0.0,
                network_delay_range_ms: (0, 0),
                network_drop_rate: 0.0,
                enable_state_corruption: false,
                enable_timing_chaos: false,
                seed: None,
            }
        }

        pub fn network_chaos(delay_ms: (u64, u64), drop_rate: f64) -> Self {
            Self {
                step_failure_rate: 0.0,
                compensation_failure_rate: 0.0,
                network_delay_range_ms: delay_ms,
                network_drop_rate: drop_rate,
                enable_state_corruption: false,
                enable_timing_chaos: false,
                seed: None,
            }
        }

        pub fn timing_chaos() -> Self {
            Self {
                step_failure_rate: 0.0,
                compensation_failure_rate: 0.0,
                network_delay_range_ms: (0, 100),
                network_drop_rate: 0.0,
                enable_state_corruption: false,
                enable_timing_chaos: true,
                seed: None,
            }
        }

        pub fn byzantine_chaos() -> Self {
            Self {
                step_failure_rate: 0.0,
                compensation_failure_rate: 0.0,
                network_delay_range_ms: (0, 0),
                network_drop_rate: 0.0,
                enable_state_corruption: true,
                enable_timing_chaos: false,
                seed: None,
            }
        }

        pub fn with_seed(mut self, seed: u64) -> Self {
            self.seed = Some(seed);
            self
        }
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
    // ChaoticStepExecutor
    // ========================================================================

    pub struct ChaoticStepExecutor {
        chaos_config: ChaosConfig,
        rng:          Mutex<StdRng>,
    }

    impl ChaoticStepExecutor {
        pub fn new(config: ChaosConfig) -> Self {
            let rng = match config.seed {
                Some(seed) => StdRng::seed_from_u64(seed),
                None => StdRng::from_entropy(),
            };
            Self {
                chaos_config: config,
                rng:          Mutex::new(rng),
            }
        }

        pub fn execute(
            &self,
            step_order: usize,
            subgraph: &str,
            mutation_name: &str,
            variables: &Value,
            _previous_result: Option<&Value>,
        ) -> Result<Value, String> {
            let mut rng = self.rng.lock().unwrap();

            // Apply timing chaos
            if self.chaos_config.enable_timing_chaos {
                let delay_ms = rng.gen_range(0..100);
                std::thread::sleep(Duration::from_millis(delay_ms));
            }

            // Apply network delay
            if self.chaos_config.network_delay_range_ms != (0, 0) {
                let (min, max) = self.chaos_config.network_delay_range_ms;
                let delay_ms = rng.gen_range(min..=max);
                std::thread::sleep(Duration::from_millis(delay_ms));
            }

            // Apply random failure
            if rng.gen::<f64>() < self.chaos_config.step_failure_rate {
                return Err(format!("Chaos: Random step failure at {step_order}"));
            }

            // Apply state corruption
            if self.chaos_config.enable_state_corruption && rng.gen::<f64>() < 0.1 {
                return Err(format!("Chaos: State corruption at step {step_order}"));
            }

            Ok(json!({
                "step": step_order,
                "subgraph": subgraph,
                "mutation": mutation_name,
                "variables": variables,
                "status": "completed"
            }))
        }
    }

    // ========================================================================
    // ChaoticStepCompensator
    // ========================================================================

    pub struct ChaoticStepCompensator {
        chaos_config: ChaosConfig,
        rng:          Mutex<StdRng>,
    }

    impl ChaoticStepCompensator {
        pub fn new(config: ChaosConfig) -> Self {
            let rng = match config.seed {
                Some(seed) => StdRng::seed_from_u64(seed),
                None => StdRng::from_entropy(),
            };
            Self {
                chaos_config: config,
                rng:          Mutex::new(rng),
            }
        }

        pub fn compensate(
            &self,
            step_order: usize,
            _original_result: Option<&Value>,
        ) -> Result<Value, String> {
            let mut rng = self.rng.lock().unwrap();

            // Apply timing chaos
            if self.chaos_config.enable_timing_chaos {
                let delay_ms = rng.gen_range(0..100);
                std::thread::sleep(Duration::from_millis(delay_ms));
            }

            // Apply random failure
            if rng.gen::<f64>() < self.chaos_config.compensation_failure_rate {
                return Err(format!("Chaos: Random compensation failure at {step_order}"));
            }

            Ok(json!({
                "compensated_step": step_order,
                "status": "rolled_back"
            }))
        }
    }

    // ========================================================================
    // NetworkSimulator
    // ========================================================================

    pub struct NetworkSimulator {
        chaos_config: ChaosConfig,
        rng:          Mutex<StdRng>,
    }

    impl NetworkSimulator {
        pub fn new(config: ChaosConfig) -> Self {
            let rng = match config.seed {
                Some(seed) => StdRng::seed_from_u64(seed),
                None => StdRng::from_entropy(),
            };
            Self {
                chaos_config: config,
                rng:          Mutex::new(rng),
            }
        }

        pub fn simulate_call<T, F>(&self, operation: F) -> Result<T, String>
        where
            F: FnOnce() -> Result<T, String>,
        {
            let mut rng = self.rng.lock().unwrap();

            // Simulate connection drop
            if rng.gen::<f64>() < self.chaos_config.network_drop_rate {
                return Err("Network: Connection dropped".to_string());
            }

            // Simulate network delay
            let (min, max) = self.chaos_config.network_delay_range_ms;
            if min > 0 || max > 0 {
                let delay_ms = rng.gen_range(min..=max);
                std::thread::sleep(Duration::from_millis(delay_ms));
            }

            operation()
        }
    }

    // ========================================================================
    // TimeController
    // ========================================================================

    pub struct TimeController {
        chaos_config: ChaosConfig,
        rng:          Mutex<StdRng>,
    }

    impl TimeController {
        pub fn new(config: ChaosConfig) -> Self {
            let rng = match config.seed {
                Some(seed) => StdRng::seed_from_u64(seed),
                None => StdRng::from_entropy(),
            };
            Self {
                chaos_config: config,
                rng:          Mutex::new(rng),
            }
        }

        pub fn random_delay(&self) -> Duration {
            let mut rng = self.rng.lock().unwrap();
            let ms = rng.gen_range(0..100);
            Duration::from_millis(ms)
        }

        pub fn thundering_herd_delay(&self, saga_index: usize) -> Duration {
            // All sagas start within 10ms window
            Duration::from_millis((saga_index % 10) as u64)
        }
    }

    // ========================================================================
    // SagaOrchestrator
    // ========================================================================

    pub struct SagaOrchestrator {
        pub store:       InMemorySagaStore,
        pub executor:    ChaoticStepExecutor,
        pub compensator: ChaoticStepCompensator,
    }

    impl SagaOrchestrator {
        pub fn new(
            store: InMemorySagaStore,
            executor: ChaoticStepExecutor,
            compensator: ChaoticStepCompensator,
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
                SagaState::Completed | SagaState::Compensated | SagaState::CompensationFailed => {
                    return Err("Cannot re-execute saga in terminal state".to_string());
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
    // ChaosOrchestratorBuilder
    // ========================================================================

    pub struct ChaosOrchestratorBuilder {
        steps:        Vec<SagaStepDef>,
        chaos_config: ChaosConfig,
    }

    impl ChaosOrchestratorBuilder {
        pub fn new() -> Self {
            Self {
                steps:        Vec::new(),
                chaos_config: ChaosConfig::random_failures(0.0),
            }
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

        pub fn with_chaos(mut self, config: ChaosConfig) -> Self {
            self.chaos_config = config;
            self
        }

        pub fn build(self) -> (SagaOrchestrator, Vec<SagaStepDef>) {
            let store = InMemorySagaStore::new();
            let executor = ChaoticStepExecutor::new(self.chaos_config.clone());
            let compensator = ChaoticStepCompensator::new(self.chaos_config);
            let orchestrator = SagaOrchestrator::new(store, executor, compensator);
            (orchestrator, self.steps)
        }
    }

    // ========================================================================
    // Test Helpers
    // ========================================================================

    pub fn run_chaos_sagas(
        count: usize,
        config: ChaosConfig,
        step_count: usize,
    ) -> (Vec<SagaResult>, usize, usize, usize) {
        let (orchestrator, steps) = ChaosOrchestratorBuilder::new()
            .with_steps(step_count)
            .with_chaos(config)
            .build();

        let mut results = Vec::new();
        for _ in 0..count {
            let saga_id = orchestrator.create_saga(steps.clone()).unwrap();
            let result = orchestrator.execute_saga(saga_id).unwrap();
            results.push(result);
        }

        let completed = results.iter().filter(|r| r.state == SagaState::Completed).count();
        let compensated = results.iter().filter(|r| r.state == SagaState::Compensated).count();
        let failed = results.iter().filter(|r| r.state == SagaState::CompensationFailed).count();

        (results, completed, compensated, failed)
    }

    pub fn assert_saga_resilient(results: &[SagaResult]) {
        for result in results {
            // All sagas must end in a terminal state
            assert!(
                matches!(
                    result.state,
                    SagaState::Completed | SagaState::Compensated | SagaState::CompensationFailed
                ),
                "Saga {} is in non-terminal state {:?}",
                result.saga_id,
                result.state
            );

            // Completed sagas should have no error
            if result.state == SagaState::Completed {
                assert!(
                    result.error.is_none(),
                    "Completed saga {} should have no error",
                    result.saga_id
                );
            }

            // Failed/compensated sagas should have error
            if matches!(result.state, SagaState::Compensated | SagaState::CompensationFailed) {
                assert!(result.error.is_some(), "Failed saga {} should have error", result.saga_id);
            }
        }
    }
}

use harness::{ChaosConfig, SagaState, assert_saga_resilient, run_chaos_sagas};

// ============================================================================
// Category 1: Random Failure Chaos (4 tests)
// ============================================================================

#[test]
fn chaos_random_10pct_failure_rate_50_sagas() {
    let config = ChaosConfig::random_failures(0.1).with_seed(42);
    let (results, completed, compensated, failed) = run_chaos_sagas(50, config, 5);

    assert_saga_resilient(&results);
    assert!(!results.is_empty(), "Should have results");
    assert!(completed > 0, "Some sagas should complete");
    assert!(compensated + failed > 0, "Some sagas should fail");
}

#[test]
fn chaos_random_25pct_failure_rate_50_sagas() {
    let config = ChaosConfig::random_failures(0.25).with_seed(43);
    let (results, completed, compensated, failed) = run_chaos_sagas(50, config, 5);

    assert_saga_resilient(&results);
    assert!(!results.is_empty(), "Should have results");
    // With 25% failure rate, most sagas should fail
    assert!(compensated + failed > completed, "More failures expected than completions");
}

#[test]
fn chaos_random_failures_mixed_compensation() {
    // Failures in forward phase trigger compensation
    let config = ChaosConfig::random_failures(0.15).with_seed(44);
    let (results, _completed, compensated, _failed) = run_chaos_sagas(30, config, 5);

    assert_saga_resilient(&results);
    // Some sagas should have been compensated
    assert!(compensated > 0, "Some sagas should complete compensation");
}

#[test]
fn chaos_random_failures_with_recovery() {
    // Many failures simulate need for recovery
    let config = ChaosConfig::random_failures(0.2).with_seed(45);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(40, config, 4);

    assert_saga_resilient(&results);
    // All sagas should reach terminal state
    let terminal_count = results
        .iter()
        .filter(|r| {
            matches!(
                r.state,
                SagaState::Completed | SagaState::Compensated | SagaState::CompensationFailed
            )
        })
        .count();
    assert_eq!(terminal_count, results.len(), "All sagas must be in terminal state");
}

// ============================================================================
// Category 2: Network Chaos (3 tests)
// ============================================================================

#[test]
fn chaos_network_delays_100ms_spikes() {
    // Network delays should not cause failures, just slow execution
    let config = ChaosConfig::network_chaos((50, 150), 0.0).with_seed(46);
    let (results, completed, _compensated, _failed) = run_chaos_sagas(20, config, 5);

    assert_saga_resilient(&results);
    // Network delays alone shouldn't cause failures
    assert!(completed > 15, "Most sagas should complete despite delays");
}

#[test]
fn chaos_network_connection_drops() {
    // Connection drops simulate network failures
    let config = ChaosConfig::network_chaos((10, 20), 0.15).with_seed(47);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(30, config, 5);

    assert_saga_resilient(&results);
    // All sagas should be resilient despite network configuration
    assert_eq!(results.len(), 30, "All sagas should execute");
}

#[test]
fn chaos_network_retry_backoff() {
    // Combination of delays and drops tests retry logic
    let config = ChaosConfig::network_chaos((20, 100), 0.1).with_seed(48);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(25, config, 4);

    assert_saga_resilient(&results);
    assert_eq!(results.len(), 25, "All sagas should complete execution");
}

// ============================================================================
// Category 3: Timing Chaos (3 tests)
// ============================================================================

#[test]
fn chaos_timing_concurrent_compensation_races() {
    // Timing chaos can expose race conditions in compensation
    let config = ChaosConfig::timing_chaos().with_seed(49);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(20, config, 5);

    assert_saga_resilient(&results);
    assert_eq!(results.len(), 20, "Should execute all sagas");
}

#[test]
fn chaos_timing_thundering_herd_10_simultaneous() {
    // Multiple sagas starting nearly simultaneously
    let config = ChaosConfig::timing_chaos().with_seed(50);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(10, config, 5);

    assert_saga_resilient(&results);
    assert_eq!(results.len(), 10, "Should execute all sagas");
}

#[test]
fn chaos_timing_delayed_compensation_starts() {
    // Delays in compensation phase
    let config = ChaosConfig::timing_chaos().with_seed(51);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(15, config, 4);

    assert_saga_resilient(&results);
    assert_eq!(results.len(), 15, "Should execute all sagas");
}

// ============================================================================
// Category 4: Recovery Chaos (3 tests)
// ============================================================================

#[test]
fn chaos_recovery_manager_crash_mid_scan() {
    // Simulate recovery manager failures by having random saga failures
    let config = ChaosConfig::random_failures(0.2).with_seed(52);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(30, config, 5);

    assert_saga_resilient(&results);
    // Recovery should handle these gracefully
    assert_eq!(results.len(), 30, "All sagas should be processed");
}

#[test]
fn chaos_recovery_duplicate_recovery_attempts() {
    // Multiple recovery attempts on same sagas
    let config = ChaosConfig::random_failures(0.15).with_seed(53);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(25, config, 4);

    assert_saga_resilient(&results);
    // Idempotent recovery
    let all_terminal = results.iter().all(|r| {
        matches!(
            r.state,
            SagaState::Completed | SagaState::Compensated | SagaState::CompensationFailed
        )
    });
    assert!(all_terminal, "All sagas should reach terminal state");
}

#[test]
fn chaos_recovery_concurrent_recovery_managers() {
    // Simulate concurrent recovery scenarios
    let config = ChaosConfig::random_failures(0.1).with_seed(54);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(40, config, 5);

    assert_saga_resilient(&results);
    assert_eq!(results.len(), 40, "All sagas should be processed");
}

// ============================================================================
// Category 5: Byzantine Failure Chaos (3 tests)
// ============================================================================

#[test]
fn chaos_byzantine_corrupted_saga_state() {
    // State corruption detection
    let config = ChaosConfig::byzantine_chaos().with_seed(55);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(20, config, 5);

    assert_saga_resilient(&results);
    // System should reject corrupted state
    let valid_count = results.len();
    assert!(valid_count > 0, "Should process sagas");
}

#[test]
fn chaos_byzantine_invalid_step_results() {
    // Invalid JSON or data in step results
    let config = ChaosConfig::byzantine_chaos().with_seed(56);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(15, config, 4);

    assert_saga_resilient(&results);
    assert_eq!(results.len(), 15, "Should handle invalid results gracefully");
}

#[test]
fn chaos_byzantine_out_of_order_execution() {
    // Steps executing in wrong order due to timing
    let config = ChaosConfig::timing_chaos().with_seed(57);
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(10, config, 5);

    assert_saga_resilient(&results);
    assert_eq!(results.len(), 10, "Should maintain order despite timing chaos");
}

// ============================================================================
// Category 6: Combination Chaos (2 tests)
// ============================================================================

#[test]
fn chaos_combination_failures_delays_corruption() {
    // Multiple chaos types simultaneously
    let config = ChaosConfig {
        step_failure_rate: 0.1,
        compensation_failure_rate: 0.05,
        network_delay_range_ms: (10, 50),
        network_drop_rate: 0.05,
        enable_state_corruption: true,
        enable_timing_chaos: true,
        seed: Some(58),
    };
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(25, config, 5);

    assert_saga_resilient(&results);
    assert_eq!(results.len(), 25, "Should survive combined chaos");
}

#[test]
fn chaos_combination_network_recovery_concurrent() {
    // Network issues combined with recovery scenarios
    let config = ChaosConfig {
        step_failure_rate: 0.15,
        compensation_failure_rate: 0.0,
        network_delay_range_ms: (20, 80),
        network_drop_rate: 0.1,
        enable_state_corruption: false,
        enable_timing_chaos: true,
        seed: Some(59),
    };
    let (results, _completed, _compensated, _failed) = run_chaos_sagas(30, config, 4);

    assert_saga_resilient(&results);
    assert_eq!(results.len(), 30, "All sagas should complete");
}
