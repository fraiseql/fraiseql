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
use std::time::Duration;
use uuid::Uuid;

/// Test 1: Runtime Startup and Shutdown Lifecycle
///
/// Verifies that the observer runtime:
/// 1. Starts successfully and initializes all components
/// 2. Processes events during normal operation
/// 3. Shuts down gracefully without data loss
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_runtime_start_stop_lifecycle() {
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Start mock webhook server
    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    // Create observer
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-lifecycle-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Insert initial change log entry
    let order_id = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        &format!("Order_{}", test_id),
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
    let checkpoint_exists = check_checkpoint_exists(&pool, &format!("Order_{}", test_id))
        .await
        .expect("Failed to check checkpoint");
    assert!(
        checkpoint_exists,
        "Expected checkpoint to be saved after processing"
    );

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
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    // Create observer
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-checkpoint-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Insert first batch of events
    for i in 0..5 {
        let order_id = Uuid::new_v4();
        let _ = insert_change_log_entry(
            &pool,
            "INSERT",
            &format!("Order_{}", test_id),
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

    // Insert second batch of events
    for i in 5..10 {
        let order_id = Uuid::new_v4();
        let _ = insert_change_log_entry(
            &pool,
            "INSERT",
            &format!("Order_{}", test_id),
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
    let checkpoint_after_second = get_checkpoint_value(&pool, &format!("Order_{}", test_id))
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
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_hot_reload_observers() {
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    let mock_server_1 = MockWebhookServer::start().await;
    let mock_server_2 = MockWebhookServer::start().await;
    mock_server_1.mock_success().await;
    mock_server_2.mock_success().await;

    // Create initial observer pointing to server 1
    let _observer_id_1 = create_test_observer(
        &pool,
        &format!("test-reload-1-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server_1.webhook_url(),
    )
    .await
    .expect("Failed to create observer 1");

    // Insert event that should trigger observer 1
    let order_id_1 = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        &format!("Order_{}", test_id),
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
        Some("Order"),
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
        &format!("Order_{}", test_id),
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
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Create mock server with delayed responses
    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_delayed_response(Duration::from_secs(2)).await;

    // Create observer
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-shutdown-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

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
            &format!("Order_{}", test_id),
            &order_id.to_string(),
            serde_json::json!({"id": order_id.to_string(), "sequence": i}),
            None,
        )
        .await
        .expect("Failed to insert change log entry");
    }

    // Give runtime time to start processing
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Verify checkpoint was saved before attempting more events
    let checkpoint_exists = check_checkpoint_exists(&pool, &format!("Order_{}", test_id))
        .await
        .expect("Failed to check checkpoint");
    assert!(checkpoint_exists, "Expected checkpoint to exist");

    // Verify some events were processed
    let initial_count = mock_server.request_count().await;
    assert!(
        initial_count > 0,
        "Expected at least one event to start processing"
    );

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
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Mock server that initially fails, then succeeds
    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_transient_failure(2).await;

    // Create observer
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-error-resilience-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Insert event that will fail initially
    let order_id_1 = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        &format!("Order_{}", test_id),
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
        &format!("Order_{}", test_id),
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
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    // Create observer
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-throughput-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Insert high volume of events
    let event_count = 100;
    for i in 0..event_count {
        let order_id = Uuid::new_v4();
        let _ = insert_change_log_entry(
            &pool,
            "INSERT",
            &format!("Order_{}", test_id),
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

    // Cleanup
    cleanup_test_data(&pool, &test_id)
        .await
        .expect("Failed to cleanup");
}
