#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::print_stderr)] // Reason: DB-gated tests print a skip notice to stderr
use super::*;

#[test]
fn test_recovery_config_default() {
    let config = RecoveryConfig::default();
    assert_eq!(config.check_interval, Duration::from_secs(5));
    assert_eq!(config.max_sagas_per_iteration, 50);
    assert_eq!(config.stale_age_hours, 24);
}

#[test]
fn test_recovery_manager_creation() {
    // This is a basic test - full integration tests use the background_loop test file
    let config = RecoveryConfig::default();
    assert_eq!(config.check_interval, Duration::from_secs(5));
}

/// Resolve the saga-store test URL, or `None` when no database is configured.
///
/// `SagaRecoveryManager::new` requires a live `PostgresSagaStore` (its
/// constructor connects), so the fail-loud assertions below can only run when a
/// database is available. They self-skip otherwise rather than fabricate a
/// connection.
fn test_db_url() -> Option<String> {
    if std::env::var("DATABASE_URL").is_err() {
        return None;
    }
    Some(std::env::var("SAGA_STORE_TEST_URL").unwrap_or_else(|_| {
        "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql".to_string()
    }))
}

/// M-saga-recovery: `run_iteration` must fail loud — it previously transitioned
/// every Pending saga to Executing while executing nothing.
#[tokio::test]
async fn run_iteration_fails_loud() {
    let Some(url) = test_db_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };
    let store = PostgresSagaStore::new(&url).await.expect("Failed to create store");
    let manager = SagaRecoveryManager::new(Arc::new(store), RecoveryConfig::default());

    let result = manager.run_iteration().await;
    assert!(
        matches!(result, Err(SagaStoreError::NotImplemented { .. })),
        "run_iteration must fail loud, got: {result:?}"
    );
    // The running flag must never have been flipped.
    assert!(!manager.is_running(), "run_iteration must not start the loop");
}

/// M-saga-recovery: `start_background_loop` must fail loud and must not flip the
/// running flag (so it can never silently mutate saga state).
#[tokio::test]
async fn start_background_loop_fails_loud() {
    let Some(url) = test_db_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };
    let store = PostgresSagaStore::new(&url).await.expect("Failed to create store");
    let manager = SagaRecoveryManager::new(Arc::new(store), RecoveryConfig::default());

    let result = manager.start_background_loop().await;
    match result {
        Err(SagaStoreError::NotImplemented { operation }) => {
            assert_eq!(operation, "SagaRecoveryManager::start_background_loop");
        },
        other => panic!("expected NotImplemented, got: {other:?}"),
    }
    assert!(!manager.is_running(), "start_background_loop must not flip the running flag");
}
