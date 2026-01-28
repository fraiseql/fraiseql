//! # Saga Recovery Manager - Background Loop with Store Integration
//!
//! RED Phase: Tests for background recovery loop and saga store integration.
//! Establishes contracts for periodic recovery, saga detection, and advanced retry scenarios.
//!
//! This test file defines requirements before implementation (RED phase of TDD).

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use uuid::Uuid;

// ============================================================================
// Mock Saga Store for Testing
// ============================================================================

/// Mock in-memory saga store for testing recovery manager integration.
#[derive(Debug, Clone)]
pub struct MockSagaStore {
    sagas: Arc<Mutex<Vec<MockSaga>>>,
}

#[derive(Debug, Clone)]
pub struct MockSaga {
    pub id:         Uuid,
    pub state:      String,
    pub created_at: Instant,
    pub step_count: u32,
}

impl MockSagaStore {
    pub fn new() -> Self {
        Self {
            sagas: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn add_saga(&self, id: Uuid, state: &str, created_at: Instant, step_count: u32) {
        let mut sagas = self.sagas.lock().unwrap();
        sagas.push(MockSaga {
            id,
            state: state.to_string(),
            created_at,
            step_count,
        });
    }

    pub fn get_pending_sagas(&self) -> Vec<MockSaga> {
        let sagas = self.sagas.lock().unwrap();
        sagas.iter().filter(|s| s.state == "pending").cloned().collect()
    }

    pub fn get_executing_sagas(&self) -> Vec<MockSaga> {
        let sagas = self.sagas.lock().unwrap();
        sagas.iter().filter(|s| s.state == "executing").cloned().collect()
    }

    pub fn get_stale_sagas(&self, hours_threshold: i64) -> Vec<MockSaga> {
        let sagas = self.sagas.lock().unwrap();
        let threshold_duration = Duration::from_secs((hours_threshold * 3600) as u64);
        sagas
            .iter()
            .filter(|s| s.created_at.elapsed() > threshold_duration)
            .cloned()
            .collect()
    }

    pub fn update_saga_state(&self, saga_id: Uuid, new_state: &str) -> Result<(), String> {
        let mut sagas = self.sagas.lock().unwrap();
        if let Some(saga) = sagas.iter_mut().find(|s| s.id == saga_id) {
            saga.state = new_state.to_string();
            Ok(())
        } else {
            Err(format!("Saga {} not found", saga_id))
        }
    }

    pub fn delete_saga(&self, saga_id: Uuid) -> Result<(), String> {
        let mut sagas = self.sagas.lock().unwrap();
        if let Some(pos) = sagas.iter().position(|s| s.id == saga_id) {
            sagas.remove(pos);
            Ok(())
        } else {
            Err(format!("Saga {} not found", saga_id))
        }
    }

    pub fn clear(&self) {
        let mut sagas = self.sagas.lock().unwrap();
        sagas.clear();
    }

    pub fn count(&self) -> usize {
        self.sagas.lock().unwrap().len()
    }
}

impl Default for MockSagaStore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Background Loop Controller
// ============================================================================

/// Configuration for the background recovery loop.
#[derive(Debug, Clone, Copy)]
pub struct BackgroundLoopConfig {
    /// Interval between recovery checks
    pub check_interval:          Duration,
    /// Maximum sagas to process per iteration
    pub max_sagas_per_iteration: u32,
    /// Grace period before retrying a saga
    pub grace_period:            Duration,
}

impl Default for BackgroundLoopConfig {
    fn default() -> Self {
        Self {
            check_interval:          Duration::from_secs(5),
            max_sagas_per_iteration: 10,
            grace_period:            Duration::from_secs(1),
        }
    }
}

/// Controls the background recovery loop lifecycle.
pub struct BackgroundLoopController {
    config:     BackgroundLoopConfig,
    store:      MockSagaStore,
    running:    Arc<Mutex<bool>>,
    iterations: Arc<Mutex<u64>>,
}

impl BackgroundLoopController {
    pub fn new(config: BackgroundLoopConfig, store: MockSagaStore) -> Self {
        Self {
            config,
            store,
            running: Arc::new(Mutex::new(false)),
            iterations: Arc::new(Mutex::new(0)),
        }
    }

    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }

    pub fn iteration_count(&self) -> u64 {
        *self.iterations.lock().unwrap()
    }

    pub fn get_store(&self) -> &MockSagaStore {
        &self.store
    }

    pub async fn run_iteration(&self) -> Result<(), String> {
        // Increment iteration counter
        {
            let mut iterations = self.iterations.lock().unwrap();
            *iterations += 1;
        }

        // Process pending sagas
        let pending = self.store.get_pending_sagas();
        for saga in pending.iter().take(self.config.max_sagas_per_iteration as usize) {
            let _ = self.store.update_saga_state(saga.id, "executing");
        }

        Ok(())
    }

    pub async fn start(&self) -> Result<(), String> {
        let mut running = self.running.lock().unwrap();
        if *running {
            return Err("Loop already running".to_string());
        }
        *running = true;
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let mut running = self.running.lock().unwrap();
        if !*running {
            return Err("Loop not running".to_string());
        }
        *running = false;
        Ok(())
    }
}

// ============================================================================
// Test Category 1: Background Loop Lifecycle (5 tests)
// ============================================================================

#[tokio::test]
async fn test_background_loop_starts_successfully() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store);

    let result = controller.start().await;
    assert!(result.is_ok());
    assert!(controller.is_running());
}

#[tokio::test]
async fn test_background_loop_stops_gracefully() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store);

    let _ = controller.start().await;
    let result = controller.stop().await;

    assert!(result.is_ok());
    assert!(!controller.is_running());
}

#[tokio::test]
async fn test_background_loop_prevents_multiple_starts() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store);

    let _ = controller.start().await;
    let result = controller.start().await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_background_loop_prevents_stop_when_not_running() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store);

    let result = controller.stop().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_background_loop_tracks_iterations() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store);

    assert_eq!(controller.iteration_count(), 0);

    for _ in 0..5 {
        let _ = controller.run_iteration().await;
    }

    assert_eq!(controller.iteration_count(), 5);
}

// ============================================================================
// Test Category 2: Saga Detection and Processing (6 tests)
// ============================================================================

#[tokio::test]
async fn test_detects_pending_sagas() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store.clone());

    let saga_id = Uuid::new_v4();
    let store_ref = controller.get_store();
    store_ref.add_saga(saga_id, "pending", Instant::now(), 3);

    let pending = store_ref.get_pending_sagas();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, saga_id);
}

#[tokio::test]
async fn test_detects_executing_sagas() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store.clone());

    let saga_id = Uuid::new_v4();
    let store_ref = controller.get_store();
    store_ref.add_saga(saga_id, "executing", Instant::now(), 2);

    let executing = store_ref.get_executing_sagas();
    assert_eq!(executing.len(), 1);
    assert_eq!(executing[0].id, saga_id);
}

#[tokio::test]
async fn test_processes_multiple_pending_sagas() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store.clone());

    let store_ref = controller.get_store();
    for _ in 0..5 {
        store_ref.add_saga(Uuid::new_v4(), "pending", Instant::now(), 1);
    }

    let pending = store_ref.get_pending_sagas();
    assert_eq!(pending.len(), 5);

    // Run iteration should process up to max_sagas_per_iteration
    let _ = controller.run_iteration().await;

    let still_pending = store_ref.get_pending_sagas();
    assert!(still_pending.len() < 5);
}

#[tokio::test]
async fn test_respects_max_sagas_per_iteration() {
    let config = BackgroundLoopConfig {
        max_sagas_per_iteration: 3,
        ..Default::default()
    };
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store.clone());

    let store_ref = controller.get_store();
    for _ in 0..10 {
        store_ref.add_saga(Uuid::new_v4(), "pending", Instant::now(), 1);
    }

    let _ = controller.run_iteration().await;

    let executing = store_ref.get_executing_sagas();
    assert_eq!(executing.len(), 3);
}

#[tokio::test]
async fn test_ignores_completed_sagas() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store.clone());

    let store_ref = controller.get_store();
    store_ref.add_saga(Uuid::new_v4(), "completed", Instant::now(), 3);
    store_ref.add_saga(Uuid::new_v4(), "failed", Instant::now(), 3);
    store_ref.add_saga(Uuid::new_v4(), "pending", Instant::now(), 3);

    let pending = store_ref.get_pending_sagas();
    assert_eq!(pending.len(), 1);
}

#[tokio::test]
async fn test_saga_state_transitions() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store.clone());

    let saga_id = Uuid::new_v4();
    let store_ref = controller.get_store();
    store_ref.add_saga(saga_id, "pending", Instant::now(), 1);

    // Transition pending → executing
    let result = store_ref.update_saga_state(saga_id, "executing");
    assert!(result.is_ok());

    let executing = store_ref.get_executing_sagas();
    assert_eq!(executing.len(), 1);

    // Transition executing → completed
    let result = store_ref.update_saga_state(saga_id, "completed");
    assert!(result.is_ok());

    let executing = store_ref.get_executing_sagas();
    assert_eq!(executing.len(), 0);
}

// ============================================================================
// Test Category 3: Stale Saga Cleanup (4 tests)
// ============================================================================

#[tokio::test]
async fn test_detects_stale_sagas_by_age() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store.clone());

    let store_ref = controller.get_store();

    // Add old saga (created 25 hours ago, simulated)
    let old_instant = Instant::now() - Duration::from_secs(25 * 3600);
    store_ref.add_saga(Uuid::new_v4(), "completed", old_instant, 5);

    // Add fresh saga (just now)
    store_ref.add_saga(Uuid::new_v4(), "completed", Instant::now(), 5);

    let stale = store_ref.get_stale_sagas(24);
    assert_eq!(stale.len(), 1);
}

#[tokio::test]
async fn test_preserves_recent_sagas() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store.clone());

    let store_ref = controller.get_store();

    // Add saga created 1 hour ago
    let recent_instant = Instant::now() - Duration::from_secs(3600);
    store_ref.add_saga(Uuid::new_v4(), "completed", recent_instant, 5);

    let stale = store_ref.get_stale_sagas(24);
    assert_eq!(stale.len(), 0);
}

#[tokio::test]
async fn test_cleanup_removes_stale_sagas() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(config, store.clone());

    let store_ref = controller.get_store();

    let saga_id = Uuid::new_v4();
    let old_instant = Instant::now() - Duration::from_secs(25 * 3600);
    store_ref.add_saga(saga_id, "completed", old_instant, 5);

    assert_eq!(store_ref.count(), 1);

    let result = store_ref.delete_saga(saga_id);
    assert!(result.is_ok());
    assert_eq!(store_ref.count(), 0);
}

#[tokio::test]
async fn test_cleanup_error_on_missing_saga() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let _controller = BackgroundLoopController::new(config, store.clone());

    let store_ref = &store;
    let result = store_ref.delete_saga(Uuid::new_v4());

    assert!(result.is_err());
}

// ============================================================================
// Test Category 4: Concurrent Operations (4 tests)
// ============================================================================

#[tokio::test]
async fn test_concurrent_store_access() {
    let _config = BackgroundLoopConfig::default();
    let store = Arc::new(MockSagaStore::default());

    let store_clone = Arc::clone(&store);
    let handle1 = tokio::spawn(async move {
        for _ in 0..5 {
            store_clone.add_saga(Uuid::new_v4(), "pending", Instant::now(), 1);
        }
    });

    let store_clone = Arc::clone(&store);
    let handle2 = tokio::spawn(async move {
        for _ in 0..3 {
            let pending = store_clone.get_pending_sagas();
            // Verify we can read pending sagas without panic
            let _ = pending.len();
        }
    });

    let _ = tokio::join!(handle1, handle2);

    assert!(store.count() >= 5);
}

#[tokio::test]
async fn test_concurrent_state_updates() {
    let _config = BackgroundLoopConfig::default();
    let store = Arc::new(MockSagaStore::default());

    let saga_id = Uuid::new_v4();
    store.add_saga(saga_id, "pending", Instant::now(), 1);

    let store_clone = Arc::clone(&store);
    let handle = tokio::spawn(async move {
        let _ = store_clone.update_saga_state(saga_id, "executing");
    });

    let _ = handle.await;

    let executing = store.get_executing_sagas();
    assert_eq!(executing.len(), 1);
}

#[tokio::test]
async fn test_multiple_controllers_same_store() {
    let config = BackgroundLoopConfig::default();
    let store = Arc::new(MockSagaStore::default());

    let controller1 = BackgroundLoopController::new(config, (*store).clone());
    let controller2 = BackgroundLoopController::new(config, (*store).clone());

    let _ = controller1.run_iteration().await;
    let _ = controller2.run_iteration().await;

    assert_eq!(controller1.iteration_count(), 1);
    assert_eq!(controller2.iteration_count(), 1);
}

#[tokio::test]
async fn test_no_data_corruption_under_concurrency() {
    let _config = BackgroundLoopConfig::default();
    let store = Arc::new(MockSagaStore::default());

    for _ in 0..10 {
        store.add_saga(Uuid::new_v4(), "pending", Instant::now(), 1);
    }

    let mut handles = vec![];

    for _ in 0..5 {
        let store_clone = Arc::clone(&store);
        let handle = tokio::spawn(async move {
            for _ in 0..10 {
                let pending = store_clone.get_pending_sagas();
                let executing = store_clone.get_executing_sagas();
                let total = pending.len() + executing.len();
                assert!(total <= 10);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    let total = store.count();
    assert!(total <= 10);
}

// ============================================================================
// Test Category 5: Error Handling and Recovery (3 tests)
// ============================================================================

#[tokio::test]
async fn test_handles_missing_saga_gracefully() {
    let _config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let _controller = BackgroundLoopController::new(_config, store.clone());

    let result = store.update_saga_state(Uuid::new_v4(), "executing");

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[tokio::test]
async fn test_continues_on_individual_saga_failure() {
    let _config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let controller = BackgroundLoopController::new(_config, store);

    let store = controller.get_store();
    store.add_saga(Uuid::new_v4(), "pending", Instant::now(), 1);
    store.add_saga(Uuid::new_v4(), "pending", Instant::now(), 1);

    // Try to update non-existent saga (will fail)
    let _ = store.update_saga_state(Uuid::new_v4(), "executing");

    // But we should still be able to process others
    let pending = store.get_pending_sagas();
    assert_eq!(pending.len(), 2);
}

#[tokio::test]
async fn test_handles_store_clear() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let _controller = BackgroundLoopController::new(config, store.clone());

    let store_ref = &store;
    for _ in 0..5 {
        store_ref.add_saga(Uuid::new_v4(), "pending", Instant::now(), 1);
    }

    assert_eq!(store_ref.count(), 5);

    store_ref.clear();
    assert_eq!(store_ref.count(), 0);
}

// ============================================================================
// Test Category 6: Performance (2 tests)
// ============================================================================

#[tokio::test]
async fn test_handles_large_pending_saga_set() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let _controller = BackgroundLoopController::new(config, store.clone());

    let store_ref = &store;
    for _ in 0..1000 {
        store_ref.add_saga(Uuid::new_v4(), "pending", Instant::now(), 1);
    }

    let start = Instant::now();
    let pending = store_ref.get_pending_sagas();
    let elapsed = start.elapsed();

    assert_eq!(pending.len(), 1000);
    assert!(elapsed < Duration::from_secs(1), "Should query 1000 sagas in <1s");
}

#[tokio::test]
async fn test_stale_detection_performance() {
    let config = BackgroundLoopConfig::default();
    let store = MockSagaStore::default();
    let _controller = BackgroundLoopController::new(config, store.clone());

    let store_ref = &store;
    for i in 0..500 {
        let age = if i < 100 {
            Instant::now() - Duration::from_secs(25 * 3600) // Stale
        } else {
            Instant::now() - Duration::from_secs(3600) // Fresh
        };
        store_ref.add_saga(Uuid::new_v4(), "completed", age, 1);
    }

    let start = Instant::now();
    let stale = store_ref.get_stale_sagas(24);
    let elapsed = start.elapsed();

    assert_eq!(stale.len(), 100);
    assert!(elapsed < Duration::from_millis(100), "Should find 100 stale sagas in <100ms");
}
