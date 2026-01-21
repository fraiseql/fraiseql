use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use fraiseql_runtime::lifecycle::shutdown::{ShutdownCoordinator, ShutdownConfig};

#[tokio::test]
async fn test_graceful_shutdown_waits_for_requests() {
    let config = ShutdownConfig {
        timeout: Duration::from_secs(5),
        delay: Duration::from_millis(100),
    };
    let coordinator = ShutdownCoordinator::new(config);

    // Simulate an in-flight request
    let guard = coordinator.request_started().unwrap();
    assert_eq!(coordinator.in_flight_count(), 1);

    // Start shutdown in background
    let shutdown_coordinator = coordinator.clone();
    let shutdown_handle = tokio::spawn(async move {
        shutdown_coordinator.shutdown().await;
    });

    // Wait a bit for shutdown to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Should be shutting down
    assert!(coordinator.is_shutting_down());

    // Complete the request
    drop(guard);
    assert_eq!(coordinator.in_flight_count(), 0);

    // Shutdown should complete
    timeout(Duration::from_secs(1), shutdown_handle)
        .await
        .expect("Shutdown should complete")
        .expect("Shutdown task should not panic");
}

#[tokio::test]
async fn test_shutdown_rejects_new_requests() {
    let config = ShutdownConfig {
        timeout: Duration::from_secs(1),
        delay: Duration::from_millis(0),
    };
    let coordinator = ShutdownCoordinator::new(config);

    // Start shutdown
    let shutdown_coordinator = coordinator.clone();
    tokio::spawn(async move {
        shutdown_coordinator.shutdown().await;
    });

    // Wait for shutdown to initiate
    tokio::time::sleep(Duration::from_millis(50)).await;

    // New requests should be rejected
    assert!(coordinator.request_started().is_none());
}

#[tokio::test]
async fn test_readiness_changes_on_shutdown() {
    let config = ShutdownConfig::default();
    let coordinator = ShutdownCoordinator::new(config);

    assert!(coordinator.is_ready());

    let shutdown_coordinator = coordinator.clone();
    tokio::spawn(async move {
        shutdown_coordinator.shutdown().await;
    });

    // Wait for readiness to change
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(!coordinator.is_ready());
}

#[tokio::test]
async fn test_shutdown_timeout_with_remaining_requests() {
    let config = ShutdownConfig {
        timeout: Duration::from_millis(100),
        delay: Duration::from_millis(0),
    };
    let coordinator = ShutdownCoordinator::new(config);

    // Create a request that won't complete
    let _guard = coordinator.request_started().unwrap();
    assert_eq!(coordinator.in_flight_count(), 1);

    // Start shutdown
    let start = std::time::Instant::now();
    coordinator.shutdown().await;
    let elapsed = start.elapsed();

    // Should have timed out after ~100ms
    assert!(elapsed >= Duration::from_millis(100));
    assert!(elapsed < Duration::from_millis(500));

    // Request should still be in flight
    assert_eq!(coordinator.in_flight_count(), 1);
}
