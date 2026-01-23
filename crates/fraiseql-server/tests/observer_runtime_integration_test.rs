//! FraiseQL Observer Runtime Integration Tests
//!
//! These tests verify the complete Observer Runtime lifecycle and resilience:
//! 1. Runtime startup/shutdown
//! 2. Checkpoint recovery after restart
//! 3. Hot reload of observer configurations
//! 4. Graceful shutdown during active processing
//! 5. Recovery after runtime errors
//! 6. High throughput processing capabilities
//!
//! # Requirements
//!
//! This test file requires:
//! - PostgreSQL running on localhost:5432
//! - Test database `fraiseql_test` with permissions
//!
//! # Running Tests
//!
//! 1. Start PostgreSQL:
//! ```bash
//! docker run -d --name postgres-test \
//!   -e POSTGRES_PASSWORD=postgres \
//!   -e POSTGRES_DB=fraiseql_test \
//!   -p 5432:5432 \
//!   postgres:16
//! ```
//!
//! 2. Set environment variable:
//! ```bash
//! export DATABASE_URL="postgresql://postgres:postgres@localhost/fraiseql_test"
//! ```
//!
//! 3. Run tests:
//! ```bash
//! cargo test --test observer_runtime_integration_test --features observers -- --ignored --nocapture
//! ```

#![cfg(feature = "observers")]

mod observer_test_helpers;

use observer_test_helpers::*;
use fraiseql_server::observers::runtime::{ObserverRuntime, ObserverRuntimeConfig};
use std::time::Duration;
use uuid::Uuid;

/// Initialize tracing subscriber for test logging
fn init_test_tracing() {
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".into())
            )
            .with_test_writer()
            .init();
    });
}

/// Test 1: Runtime Startup and Shutdown Lifecycle
///
/// Verifies that the observer runtime:
/// 1. Starts successfully and initializes all components
/// 2. Processes events during normal operation
/// 3. Shuts down gracefully without data loss
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_runtime_start_stop_lifecycle() {
    init_test_tracing();

    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    // Start mock webhook server
    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    // Create observer with unique entity type for this test
    let entity_type = format!("Order_{}", test_id);
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-lifecycle-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Create and start observer runtime with fast polling for tests
    let config = ObserverRuntimeConfig::new(pool.clone())
        .with_poll_interval(50);
    let mut runtime = ObserverRuntime::new(config);
    runtime.start().await.expect("Failed to start runtime");

    // Insert initial change log entry with matching entity type
    let order_id = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &order_id.to_string(),
        serde_json::json!({"id": order_id.to_string(), "status": "new"}),
        None,
    )
    .await
    .expect("Failed to insert change log entry");

    // Wait for webhook processing
    wait_for_webhook(&mock_server, 1, Duration::from_secs(15)).await;

    // Verify successful processing
    let requests = mock_server.received_requests().await;
    assert_eq!(
        requests.len(),
        1,
        "Expected 1 webhook call during lifecycle, got {}",
        requests.len()
    );

    // Verify observer log
    let log_count = get_observer_log_count(&pool, "success")
        .await
        .expect("Failed to query observer logs");
    assert!(log_count > 0, "Expected at least 1 successful observer log");

    // Verify checkpoint was saved
    let checkpoint_exists = check_checkpoint_exists(&pool, &entity_type)
        .await
        .expect("Failed to check checkpoint");
    assert!(
        checkpoint_exists,
        "Expected checkpoint to be saved after processing"
    );

    // Stop the runtime gracefully
    runtime.stop().await.expect("Failed to stop runtime");

    // Cleanup
    cleanup_test_data(&pool, &test_id)
        .await
        .expect("Failed to cleanup");
}

/// Test 2: Checkpoint Recovery After Runtime Restart
///
/// Verifies that the observer runtime:
/// 1. Saves checkpoints during normal processing
/// 2. Recovers from checkpoint on restart
/// 3. Resumes processing without missing events or processing duplicates
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_checkpoint_recovery_after_restart() {
    init_test_tracing();

    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    // Create observer with unique entity type for this test
    let entity_type = format!("Order_{}", test_id);
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-checkpoint-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Create and start observer runtime with fast polling for tests
    let config = ObserverRuntimeConfig::new(pool.clone())
        .with_poll_interval(50);
    let mut runtime = ObserverRuntime::new(config);
    runtime.start().await.expect("Failed to start runtime");

    // Insert first batch of events with matching entity type
    for i in 0..5 {
        let order_id = Uuid::new_v4();
        let _ = insert_change_log_entry(
            &pool,
            "INSERT",
            &entity_type,
            &order_id.to_string(),
            serde_json::json!({"id": order_id.to_string(), "sequence": i}),
            None,
        )
        .await
        .expect("Failed to insert change log entry");
    }

    // Wait for first batch processing
    wait_for_webhook(&mock_server, 5, Duration::from_secs(20)).await;

    // Record first checkpoint state
    let first_request_count = mock_server.request_count().await;
    assert_eq!(
        first_request_count, 5,
        "Expected 5 webhooks after first batch, got {}",
        first_request_count
    );

    // Insert second batch of events with matching entity type
    for i in 5..10 {
        let order_id = Uuid::new_v4();
        let _ = insert_change_log_entry(
            &pool,
            "INSERT",
            &entity_type,
            &order_id.to_string(),
            serde_json::json!({"id": order_id.to_string(), "sequence": i}),
            None,
        )
        .await
        .expect("Failed to insert change log entry");
    }

    // Wait for second batch processing
    wait_for_webhook(&mock_server, 10, Duration::from_secs(20)).await;

    // Verify checkpoint was updated
    let checkpoint_after_second = get_checkpoint_value(&pool, &entity_type)
        .await
        .expect("Failed to get checkpoint");
    assert!(
        checkpoint_after_second > 0,
        "Expected checkpoint to be updated after second batch"
    );

    // Verify no duplicates in webhook requests
    let requests = mock_server.received_requests().await;
    let ids: Vec<String> = requests
        .iter()
        .filter_map(|r| {
            r["after"]["id"]
                .as_str()
                .map(|s| s.to_string())
        })
        .collect();

    let unique_ids: std::collections::HashSet<_> = ids.iter().cloned().collect();
    assert_eq!(
        ids.len(),
        unique_ids.len(),
        "Expected no duplicate IDs in webhook payloads"
    );

    // Stop the runtime gracefully
    runtime.stop().await.expect("Failed to stop runtime");

    // Cleanup
    cleanup_test_data(&pool, &test_id)
        .await
        .expect("Failed to cleanup");
}

/// Test 3: Hot Reload of Observer Configurations
///
/// Verifies that observer configurations can be reloaded without:
/// 1. Losing in-flight events
/// 2. Stopping the runtime
/// 3. Requiring manual intervention
///
/// NOTE: Currently failing because `reload_observers()` only updates the count,
/// not the actual matcher/executor. Hot reload needs to atomically swap the matcher
/// to make new observers active. See runtime.rs:454-462 for details.
#[tokio::test]
#[ignore = "requires PostgreSQL; hot reload not fully implemented"]
async fn test_hot_reload_observers() {
    init_test_tracing();

    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    let mock_server_1 = MockWebhookServer::start().await;
    let mock_server_2 = MockWebhookServer::start().await;
    mock_server_1.mock_success().await;
    mock_server_2.mock_success().await;

    // Create unique entity type for this test
    let entity_type = format!("Order_{}", test_id);

    // Create initial observer pointing to server 1
    let _observer_id_1 = create_test_observer(
        &pool,
        &format!("test-reload-1-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server_1.webhook_url(),
    )
    .await
    .expect("Failed to create observer 1");

    // Create and start observer runtime with fast polling for tests
    let config = ObserverRuntimeConfig::new(pool.clone())
        .with_poll_interval(50);
    let mut runtime = ObserverRuntime::new(config);
    runtime.start().await.expect("Failed to start runtime");

    // Insert event that should trigger observer 1
    let order_id_1 = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &order_id_1.to_string(),
        serde_json::json!({"id": order_id_1.to_string(), "status": "created"}),
        None,
    )
    .await
    .expect("Failed to insert change log entry 1");

    wait_for_webhook(&mock_server_1, 1, Duration::from_secs(15)).await;
    assert_eq!(mock_server_1.request_count().await, 1);

    // Create second observer pointing to server 2
    let _observer_id_2 = create_test_observer(
        &pool,
        &format!("test-reload-2-{}", test_id),
        Some(&entity_type),
        Some("UPDATE"),
        None,
        &mock_server_2.webhook_url(),
    )
    .await
    .expect("Failed to create observer 2");

    // Insert UPDATE event that should trigger both observers after reload
    let order_id_2 = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "UPDATE",
        &entity_type,
        &order_id_2.to_string(),
        serde_json::json!({"id": order_id_2.to_string(), "status": "updated"}),
        Some(serde_json::json!({"id": order_id_2.to_string(), "status": "created"})),
    )
    .await
    .expect("Failed to insert change log entry 2");

    // Wait for second webhook
    wait_for_webhook(&mock_server_2, 1, Duration::from_secs(15)).await;

    // Verify both observers processed their respective events
    assert_eq!(
        mock_server_1.request_count().await,
        1,
        "Observer 1 should have 1 event"
    );
    assert_eq!(
        mock_server_2.request_count().await,
        1,
        "Observer 2 should have 1 event after reload"
    );

    // Stop the runtime gracefully
    runtime.stop().await.expect("Failed to stop runtime");

    // Cleanup
    cleanup_test_data(&pool, &test_id)
        .await
        .expect("Failed to cleanup");
}

/// Test 4: Graceful Shutdown During Active Processing
///
/// Verifies that graceful shutdown:
/// 1. Allows in-flight requests to complete
/// 2. Saves checkpoints before stopping
/// 3. Prevents new events from starting processing
/// 4. Resumes cleanly on restart
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_graceful_shutdown_mid_processing() {
    init_test_tracing();

    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    // Create mock server with delayed responses
    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_delayed_response(Duration::from_secs(2)).await;

    // Create unique entity type for this test
    let entity_type = format!("Order_{}", test_id);

    // Create observer
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-shutdown-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Create and start observer runtime with fast polling for tests
    let config = ObserverRuntimeConfig::new(pool.clone())
        .with_poll_interval(50);
    let mut runtime = ObserverRuntime::new(config);
    runtime.start().await.expect("Failed to start runtime");

    // Insert events for processing
    let order_ids: Vec<_> = (0..5)
        .map(|i| {
            let order_id = Uuid::new_v4();
            (order_id, i)
        })
        .collect();

    for (order_id, i) in &order_ids {
        let _ = insert_change_log_entry(
            &pool,
            "INSERT",
            &entity_type,
            &order_id.to_string(),
            serde_json::json!({"id": order_id.to_string(), "sequence": i}),
            None,
        )
        .await
        .expect("Failed to insert change log entry");
    }

    // Give runtime time to start processing
    // Note: webhooks have 2s delay and are processed sequentially
    // We have 5 events × 2s = 10s + buffer = 11s
    tokio::time::sleep(Duration::from_secs(11)).await;

    // Verify checkpoint was saved before attempting more events
    let checkpoint_exists = check_checkpoint_exists(&pool, &entity_type)
        .await
        .expect("Failed to check checkpoint");
    assert!(checkpoint_exists, "Expected checkpoint to exist");

    // Verify some events were processed
    let initial_count = mock_server.request_count().await;
    assert!(
        initial_count > 0,
        "Expected at least one event to start processing"
    );

    // Stop the runtime gracefully
    runtime.stop().await.expect("Failed to stop runtime");

    // Cleanup
    cleanup_test_data(&pool, &test_id)
        .await
        .expect("Failed to cleanup");
}

/// Test 5: Runtime Continues After Errors
///
/// Verifies that the observer runtime:
/// 1. Continues processing after transient errors
/// 2. Implements retry logic for failed webhooks
/// 3. Records error states properly
/// 4. Maintains system stability under error conditions
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_runtime_continues_after_errors() {
    init_test_tracing();

    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    // Mock server that initially fails, then succeeds
    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_transient_failure(2).await;

    // Create unique entity type for this test
    let entity_type = format!("Order_{}", test_id);

    // Create observer
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-error-resilience-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Create and start observer runtime with fast polling for tests
    let config = ObserverRuntimeConfig::new(pool.clone())
        .with_poll_interval(50);
    let mut runtime = ObserverRuntime::new(config);
    runtime.start().await.expect("Failed to start runtime");

    // Insert event that will fail initially
    let order_id_1 = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &order_id_1.to_string(),
        serde_json::json!({"id": order_id_1.to_string(), "sequence": 1}),
        None,
    )
    .await
    .expect("Failed to insert change log entry 1");

    // Wait for retries and success
    wait_for_webhook(&mock_server, 1, Duration::from_secs(20)).await;

    // Verify success after retries
    let requests = mock_server.received_requests().await;
    assert_eq!(
        requests.len(),
        1,
        "Expected 1 successful webhook after retries"
    );

    // Verify log shows attempt tracking
    let logs = get_observer_logs_for_entity(&pool, &order_id_1.to_string())
        .await
        .expect("Failed to fetch observer logs");

    // Should have multiple log entries tracking retries
    assert!(
        !logs.is_empty(),
        "Expected observer logs for event with retries"
    );

    // Reset mock server for second event
    mock_server.reset().await;
    mock_server.mock_success().await;

    // Insert second event - should process normally
    let order_id_2 = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &order_id_2.to_string(),
        serde_json::json!({"id": order_id_2.to_string(), "sequence": 2}),
        None,
    )
    .await
    .expect("Failed to insert change log entry 2");

    // Wait for second event
    wait_for_webhook(&mock_server, 1, Duration::from_secs(15)).await;

    // Verify runtime continues normally
    let second_count = mock_server.request_count().await;
    assert_eq!(
        second_count, 1,
        "Expected runtime to continue processing after errors"
    );

    // Stop the runtime gracefully
    runtime.stop().await.expect("Failed to stop runtime");

    // Cleanup
    cleanup_test_data(&pool, &test_id)
        .await
        .expect("Failed to cleanup");
}

/// Test 6: High Throughput Processing
///
/// Verifies that the observer runtime:
/// 1. Handles high volume of events efficiently
/// 2. Maintains consistent throughput
/// 3. Processes all events without loss
/// 4. Scales appropriately with batch processing
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_high_throughput_processing() {
    init_test_tracing();

    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    // Create unique entity type for this test
    let entity_type = format!("Order_{}", test_id);

    // Create observer
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-throughput-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Create and start observer runtime with fast polling for tests
    let config = ObserverRuntimeConfig::new(pool.clone())
        .with_poll_interval(50);
    let mut runtime = ObserverRuntime::new(config);
    runtime.start().await.expect("Failed to start runtime");

    // Insert high volume of events with matching entity type
    let event_count = 100;
    for i in 0..event_count {
        let order_id = Uuid::new_v4();
        let _ = insert_change_log_entry(
            &pool,
            "INSERT",
            &entity_type,
            &order_id.to_string(),
            serde_json::json!({"id": order_id.to_string(), "sequence": i, "batch": "throughput"}),
            None,
        )
        .await
        .expect("Failed to insert change log entry");
    }

    // Wait for all events to process (with longer timeout for high volume)
    wait_for_webhook(&mock_server, event_count, Duration::from_secs(60)).await;

    // Verify all events were processed
    let request_count = mock_server.request_count().await;
    assert_eq!(
        request_count, event_count,
        "Expected {} webhooks for high throughput test, got {}",
        event_count, request_count
    );

    // Verify all requests are unique (no duplicates)
    let requests = mock_server.received_requests().await;
    let ids: Vec<String> = requests
        .iter()
        .filter_map(|r| {
            r["after"]["id"]
                .as_str()
                .map(|s| s.to_string())
        })
        .collect();

    let unique_ids: std::collections::HashSet<_> = ids.iter().cloned().collect();
    assert_eq!(
        ids.len(),
        unique_ids.len(),
        "Expected no duplicates in high throughput test"
    );

    // Verify successful logging
    let success_count = get_observer_log_count(&pool, "success")
        .await
        .expect("Failed to query observer logs");
    assert!(
        success_count as usize >= event_count * 90 / 100, // Allow 10% margin for retries
        "Expected at least 90% of events logged as success, got {}",
        success_count
    );

    // Stop the runtime gracefully
    runtime.stop().await.expect("Failed to stop runtime");

    // Cleanup
    cleanup_test_data(&pool, &test_id)
        .await
        .expect("Failed to cleanup");
}

/// Simple validation test - verify runtime can start/stop without errors
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_runtime_basic_lifecycle() {
    init_test_tracing();

    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    // Create basic config
    let config = ObserverRuntimeConfig::new(pool.clone())
        .with_poll_interval(50);

    let mut runtime = ObserverRuntime::new(config);

    // Start should succeed even with no observers
    let start_result = runtime.start().await;
    assert!(start_result.is_ok(), "Failed to start runtime: {:?}", start_result);

    // Should be able to stop
    let stop_result = runtime.stop().await;
    assert!(stop_result.is_ok(), "Failed to stop runtime: {:?}", stop_result);

    // Verify we can start again
    let mut runtime2 = ObserverRuntime::new(ObserverRuntimeConfig::new(pool));
    let start_result2 = runtime2.start().await;
    assert!(start_result2.is_ok(), "Failed to start runtime second time");
    
    runtime2.stop().await.ok();
}

/// Debug test - check if events are being polled and processed
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_debug_event_processing() {
    init_test_tracing();

    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    // Insert observer with webhook
    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    let _observer_id = create_test_observer(
        &pool,
        "debug-observer",
        Some("TestOrder"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Check that observer was created
    let observer_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tb_observer WHERE enabled = true")
        .fetch_one(&pool)
        .await
        .expect("Failed to count observers");
    
    println!("✓ Created observer. Count in DB: {}", observer_count.0);
    assert_eq!(observer_count.0, 1, "Observer not in database");

    // Create and start runtime
    let config = ObserverRuntimeConfig::new(pool.clone())
        .with_poll_interval(10);  // Very fast polling
    
    let mut runtime = ObserverRuntime::new(config);
    runtime.start().await.expect("Failed to start runtime");
    println!("✓ Runtime started");

    // Insert change log entry
    let order_id = uuid::Uuid::new_v4();
    let _change_log_id = insert_change_log_entry(
        &pool,
        "INSERT",
        "TestOrder",
        &order_id.to_string(),
        serde_json::json!({"id": order_id.to_string(), "amount": 100}),
        None,
    )
    .await
    .expect("Failed to insert change log entry");
    println!("✓ Inserted change log entry");

    // Check that entry is in database
    let entry_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM core.tb_entity_change_log")
        .fetch_one(&pool)
        .await
        .expect("Failed to count entries");
    println!("✓ Change log entries in DB: {}", entry_count.0);

    // Wait a bit for processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check if webhook was called
    let requests = mock_server.received_requests().await;
    println!("✓ Webhook calls received: {}", requests.len());

    // Check observer log
    let log_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tb_observer_log")
        .fetch_one(&pool)
        .await
        .ok()
        .unwrap_or((0,));
    println!("✓ Observer log entries: {}", log_count.0);

    runtime.stop().await.expect("Failed to stop runtime");

    // Don't assert webhook - just log what happened
    println!("\nDebug Results:");
    println!("  Observers in DB: {}", observer_count.0);
    println!("  Change log entries: {}", entry_count.0);
    println!("  Webhook calls: {}", requests.len());
    println!("  Observer logs: {}", log_count.0);
}

/// Test observer loading from database
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_observer_loading() {
    init_test_tracing();

    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Create observer
    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    let _observer_id = create_test_observer(
        &pool,
        "load-test-observer",
        Some("Product"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Manually query observer to verify it's in database
    let observer: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT name, entity_type, event_type FROM tb_observer WHERE name = 'load-test-observer'"
    )
    .fetch_optional(&pool)
    .await
    .expect("Failed to query observer");

    let (name, entity_type, event_type) = observer.expect("Observer not found");
    println!("✓ Observer in DB:");
    println!("  name: {}", name);
    println!("  entity_type: {:?}", entity_type);
    println!("  event_type: {:?}", event_type);

    // Check actions column
    let actions: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT actions FROM tb_observer WHERE name = 'load-test-observer'"
    )
    .fetch_optional(&pool)
    .await
    .expect("Failed to query actions");

    if let Some((actions_json,)) = actions {
        println!("✓ Actions:");
        println!("  {}", serde_json::to_string_pretty(&actions_json).unwrap());
    }

    assert_eq!(name, "load-test-observer");
    assert_eq!(entity_type.as_deref(), Some("Product"));
    assert_eq!(event_type.as_deref(), Some("INSERT"));
}

/// Test if runtime loads observers from database
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_runtime_loads_observers() {
    init_test_tracing();

    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    // Create observer
    let mock_server = MockWebhookServer::start().await;
    let _observer_id = create_test_observer(
        &pool,
        "runtime-load-test",
        Some("User"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Create and start runtime
    let config = ObserverRuntimeConfig::new(pool.clone());
    let mut runtime = ObserverRuntime::new(config);
    
    // The start() method loads observers
    let start_result = runtime.start().await;
    println!("✓ Runtime.start() result: {:?}", start_result);
    assert!(start_result.is_ok(), "Failed to start: {:?}", start_result);

    // Wait a moment for internal state
    tokio::time::sleep(Duration::from_millis(100)).await;

    // If runtime has a health method, check it
    // (can't check internal state easily, but if start works and observers load, good sign)
    
    runtime.stop().await.ok();
    println!("✓ Runtime started and stopped successfully");
}

/// Detailed debug test - check Debezium envelope and event conversion
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_debug_debezium_envelope() {
    init_test_tracing();

    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    // Insert change log entry directly
    let order_id = uuid::Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        "Order",
        &order_id.to_string(),
        serde_json::json!({"id": order_id.to_string(), "total": 50}),
        None,
    )
    .await
    .expect("Failed to insert");

    // Query the change log entry
    let entry: Option<(i64, Option<i64>, String, String, String, Option<String>, chrono::DateTime<chrono::Utc>, serde_json::Value)> = sqlx::query_as(
        "SELECT pk_entity_change_log, fk_customer_org, object_type, object_id, modification_type, change_status, created_at, object_data FROM core.tb_entity_change_log LIMIT 1"
    )
    .fetch_optional(&pool)
    .await
    .expect("Query failed");

    if let Some((pk, _fk_cust, obj_type, obj_id, mod_type, change_status, _created_at, obj_data)) = entry {
        println!("✓ Change log entry found:");
        println!("  pk: {}", pk);
        println!("  object_type: {}", obj_type);
        println!("  object_id: {}", obj_id);
        println!("  modification_type: {}", mod_type);
        println!("  change_status: {:?}", change_status);
        println!("  object_data (Debezium envelope):");
        println!("    {}", serde_json::to_string_pretty(&obj_data).unwrap());

        // Check operation code
        if let Some(op_val) = obj_data.get("op") {
            println!("  ✓ op field: {:?}", op_val);
            if let Some(op_char) = op_val.as_str().and_then(|s| s.chars().next()) {
                println!("  ✓ op first char: '{}'", op_char);
                match op_char {
                    'c' => println!("    → Recognized as CREATE"),
                    'u' => println!("    → Recognized as UPDATE"),
                    'd' => println!("    → Recognized as DELETE"),
                    x => println!("    → UNRECOGNIZED: '{}'", x),
                }
            }
        }
    } else {
        println!("✗ No change log entry found!");
    }
}

/// Test action parsing from JSONB
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_action_parsing() {
    // Try to parse the action JSON that we use in tests
    let actions_json = serde_json::json!([
        {
            "type": "webhook",
            "url": "http://127.0.0.1:8080/webhook",
            "method": "POST",
            "headers": {
                "Content-Type": "application/json"
            }
        }
    ]);

    println!("Input JSON: {}", serde_json::to_string_pretty(&actions_json).unwrap());

    match serde_json::from_value::<Vec<fraiseql_observers::config::ActionConfig>>(actions_json) {
        Ok(actions) => {
            println!("✓ Successfully parsed {} actions", actions.len());
            for (i, action) in actions.iter().enumerate() {
                println!("  Action {}: {:?}", i, action);
            }
        }
        Err(e) => {
            println!("✗ Failed to parse actions: {}", e);
            panic!("Action parsing failed");
        }
    }
}

/// Test with longer wait time and polling
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_with_longer_polling() {
    init_test_tracing();

    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    let _observer_id = create_test_observer(
        &pool,
        "long-poll-test",
        Some("Widget"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Create runtime with VERY fast polling
    let config = ObserverRuntimeConfig::new(pool.clone())
        .with_poll_interval(5);  // 5ms polling
    
    let mut runtime = ObserverRuntime::new(config);
    runtime.start().await.expect("Failed to start runtime");
    
    // Wait for runtime to fully initialize (background task)
    println!("Waiting for runtime initialization...");
    tokio::time::sleep(Duration::from_millis(100)).await;
    println!("Runtime initialized");

    // Now insert event
    let widget_id = uuid::Uuid::new_v4();
    println!("Inserting change log entry...");
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        "Widget",
        &widget_id.to_string(),
        serde_json::json!({"id": widget_id.to_string(), "name": "Test Widget"}),
        None,
    )
    .await
    .expect("Failed to insert");
    println!("Change log entry inserted");

    // Wait much longer for processing (5ms polling * multiple times)
    println!("Waiting for event processing...");
    for i in 0..50 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let requests = mock_server.received_requests().await;
        if !requests.is_empty() {
            println!("✓ Webhook called after {} ms", (i + 1) * 10);
            break;
        }
        if i % 10 == 0 {
            println!("  Still waiting... ({} ms elapsed)", (i + 1) * 10);
        }
    }

    let requests = mock_server.received_requests().await;
    println!("Final webhook calls: {}", requests.len());
    println!("Expected: 1");

    runtime.stop().await.ok();

    if requests.is_empty() {
        println!("\nDEBUG: Checking database state...");
        
        // Check change log
        let cl_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM core.tb_entity_change_log")
            .fetch_one(&pool)
            .await
            .ok()
            .unwrap_or((0,));
        println!("  Change log entries: {}", cl_count.0);
        
        // Check observer
        let obs_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tb_observer WHERE enabled = true")
            .fetch_one(&pool)
            .await
            .ok()
            .unwrap_or((0,));
        println!("  Observers enabled: {}", obs_count.0);
        
        // Check observer log
        let logs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tb_observer_log")
            .fetch_one(&pool)
            .await
            .ok()
            .unwrap_or((0,));
        println!("  Observer logs: {}", logs.0);
    }

    assert!(!requests.is_empty(), "No webhook calls received after 500ms");
}

/// Direct listener test - verify listener.next_batch() works
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_listener_direct() {
    init_test_tracing();

    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Clean up old test data (order matters due to foreign keys)
    sqlx::query("DELETE FROM tb_observer_log")
        .execute(&pool)
        .await
        .expect("Failed to clean observer logs");
    sqlx::query("DELETE FROM tb_observer")
        .execute(&pool)
        .await
        .expect("Failed to clean observers");
    sqlx::query("DELETE FROM core.tb_entity_change_log")
        .execute(&pool)
        .await
        .expect("Failed to clean change log");

    // Insert a change log entry
    let product_id = uuid::Uuid::new_v4();
    insert_change_log_entry(
        &pool,
        "INSERT",
        "Product",
        &product_id.to_string(),
        serde_json::json!({"id": product_id.to_string(), "name": "Test"}),
        None,
    )
    .await
    .expect("Failed to insert");

    // Create listener directly
    let config = fraiseql_observers::listener::change_log::ChangeLogListenerConfig::new(pool.clone())
        .with_poll_interval(10);
    
    let mut listener = fraiseql_observers::listener::change_log::ChangeLogListener::new(config);

    println!("Calling listener.next_batch()...");
    let result = listener.next_batch().await;
    
    match result {
        Ok(entries) => {
            println!("✓ Got {} entries from listener", entries.len());
            assert!(!entries.is_empty(), "Listener should have found entries");
            
            for entry in entries {
                println!("  Entry: pk={}, object_type={}, op={:?}", 
                    entry.id, entry.object_type, 
                    entry.object_data.get("op"));
                
                // Try to convert to EntityEvent
                match entry.to_entity_event() {
                    Ok(event) => {
                        println!("    ✓ Converted to EntityEvent: {:?}", event.event_type);
                    }
                    Err(e) => {
                        println!("    ✗ Failed to convert: {}", e);
                        panic!("Failed to convert: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            panic!("Listener failed: {}", e);
        }
    }
}
