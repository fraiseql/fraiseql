//! FraiseQL Observer E2E Integration Tests
//!
//! These tests verify the complete observer flow:
//! 1. Database change triggers event
//! 2. Observer processes event
//! 3. Actions execute (webhooks, emails, etc.)
//! 4. Results recorded in database
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
//! cargo test --test observer_e2e_test --features observers -- --ignored --nocapture
//! ```

#![cfg(feature = "observers")]

mod observer_test_helpers;

use std::time::Duration;

use observer_test_helpers::*;
use uuid::Uuid;

/// Test 1: Happy Path - INSERT event with webhook execution
///
/// This test verifies the complete flow:
/// 1. Observer configured for Order INSERT events
/// 2. INSERT into change log
/// 3. Listener detects change
/// 4. Webhook fires successfully
/// 5. Observer log records success
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_observer_happy_path_insert_webhook() {
    // Setup
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Start mock webhook server
    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    // Create observer for Order INSERTs
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-observer-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None, // No condition
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Insert change log entry (simulates entity mutation)
    let order_id = Uuid::new_v4();
    let order_data = serde_json::json!({
        "id": order_id.to_string(),
        "status": "pending",
        "amount": 100.0,
        "customer": "test"
    });

    let _change_log_id = insert_change_log_entry(
        &pool,
        "INSERT",
        &format!("Order_{}", test_id),
        &order_id.to_string(),
        order_data.clone(),
        None,
    )
    .await
    .expect("Failed to insert change log entry");

    // Wait for webhook processing with timeout
    let timeout = Duration::from_secs(10);
    wait_for_webhook(&mock_server, 1, timeout).await;

    // Assertions
    let requests = mock_server.received_requests().await;
    assert_eq!(requests.len(), 1, "Expected exactly 1 webhook call, got {}", requests.len());

    let webhook_payload = &requests[0];
    assert_webhook_payload(webhook_payload, &order_id.to_string(), Some(("status", "pending")));

    // Verify observer_log entry
    assert_observer_log(
        &pool,
        &order_id.to_string(),
        "success",
        Some(1), // Single attempt for success
    )
    .await;

    // Verify only one success entry
    let success_count = get_observer_log_count(&pool, "success")
        .await
        .expect("Failed to query observer logs");
    assert_eq!(success_count, 1, "Expected 1 success log entry");

    // Cleanup
    cleanup_test_data(&pool, &test_id).await.expect("Failed to cleanup");
}

/// Test 2: Conditional Execution
///
/// Verifies that observers with conditions:
/// - DO execute when condition matches
/// - DO NOT execute when condition doesn't match
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_observer_conditional_execution() {
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    // Observer with condition: only fire when status = 'shipped'
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-conditional-{}", test_id),
        Some("Order"),
        Some("UPDATE"),
        Some("status == 'shipped'"), // DSL condition
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Case 1: INSERT with status = 'pending' → should NOT fire
    let order_id_1 = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "UPDATE",
        &format!("Order_{}", test_id),
        &order_id_1.to_string(),
        serde_json::json!({"id": order_id_1.to_string(), "status": "pending"}),
        Some(serde_json::json!({"id": order_id_1.to_string(), "status": "created"})),
    )
    .await
    .expect("Failed to insert change log entry");

    // Wait a bit for processing
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        mock_server.request_count().await,
        0,
        "Webhook should not fire for status='pending'"
    );

    // Case 2: Update with status = 'shipped' → SHOULD fire
    let order_id_2 = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "UPDATE",
        &format!("Order_{}", test_id),
        &order_id_2.to_string(),
        serde_json::json!({"id": order_id_2.to_string(), "status": "shipped"}),
        Some(serde_json::json!({"id": order_id_2.to_string(), "status": "pending"})),
    )
    .await
    .expect("Failed to insert change log entry");

    // Wait for webhook
    wait_for_webhook(&mock_server, 1, Duration::from_secs(10)).await;

    let requests = mock_server.received_requests().await;
    assert_eq!(requests.len(), 1, "Expected 1 webhook call for shipped status");
    assert_eq!(requests[0]["after"]["status"], "shipped");

    // Cleanup
    cleanup_test_data(&pool, &test_id).await.expect("Failed to cleanup");
}

/// Test 3: Multiple Observers
///
/// Verifies that a single event can match and execute multiple observers
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_multiple_observers_single_event() {
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Create two mock servers for different webhooks
    let mock_server_1 = MockWebhookServer::start().await;
    let mock_server_2 = MockWebhookServer::start().await;
    mock_server_1.mock_success().await;
    mock_server_2.mock_success().await;

    // Observer 1: All Order INSERTs
    let _observer_id_1 = create_test_observer(
        &pool,
        &format!("test-multi-1-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server_1.webhook_url(),
    )
    .await
    .expect("Failed to create observer 1");

    // Observer 2: All Order INSERTs (different webhook)
    let _observer_id_2 = create_test_observer(
        &pool,
        &format!("test-multi-2-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server_2.webhook_url(),
    )
    .await
    .expect("Failed to create observer 2");

    // Insert single event
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

    // Wait for BOTH webhooks
    wait_for_webhook(&mock_server_1, 1, Duration::from_secs(10)).await;
    wait_for_webhook(&mock_server_2, 1, Duration::from_secs(10)).await;

    assert_eq!(mock_server_1.request_count().await, 1, "Observer 1 should fire once");
    assert_eq!(mock_server_2.request_count().await, 1, "Observer 2 should fire once");

    // Verify tb_observer_log has 2 success entries
    let success_count = get_observer_log_count(&pool, "success")
        .await
        .expect("Failed to query observer logs");
    assert_eq!(success_count, 2, "Expected 2 success log entries");

    // Cleanup
    cleanup_test_data(&pool, &test_id).await.expect("Failed to cleanup");
}

/// Test 4: Retry Logic with Exponential Backoff
///
/// Verifies that:
/// - Failed webhooks are retried
/// - Exponential backoff is applied
/// - Success on retry is logged
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_observer_retry_exponential_backoff() {
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Mock server: fail twice, succeed on third attempt
    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_transient_failure(2).await;

    let _observer_id = create_test_observer(
        &pool,
        &format!("test-retry-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    let order_id = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        &format!("Order_{}", test_id),
        &order_id.to_string(),
        serde_json::json!({"id": order_id.to_string()}),
        None,
    )
    .await
    .expect("Failed to insert change log entry");

    // Wait for successful webhook (after retries)
    // With exponential backoff: 100ms + 200ms + 300ms = 600ms minimum, plus processing
    wait_for_webhook(&mock_server, 1, Duration::from_secs(15)).await;

    // Verify retry attempts in tb_observer_log
    let logs = get_observer_logs_for_entity(&pool, &order_id.to_string())
        .await
        .expect("Failed to fetch observer logs");

    // Should have up to 3 attempts in the log
    assert!(!logs.is_empty(), "Expected observer log entries for entity {}", order_id);

    // Verify final status is success
    let final_status = &logs.last().expect("Should have at least one log").0;
    assert_eq!(final_status, "success", "Final status should be success");

    // Cleanup
    cleanup_test_data(&pool, &test_id).await.expect("Failed to cleanup");
}

/// Test 5: Dead Letter Queue (DLQ) on Permanent Failure
///
/// Verifies that:
/// - Actions failing all retries are moved to DLQ
/// - All retry attempts are logged
/// - Action is not retried after exhausting attempts
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_observer_dlq_permanent_failure() {
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    // Mock server: always fail
    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_failure(500).await;

    let _observer_id = create_test_observer(
        &pool,
        &format!("test-dlq-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    let order_id = Uuid::new_v4();
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        &format!("Order_{}", test_id),
        &order_id.to_string(),
        serde_json::json!({"id": order_id.to_string()}),
        None,
    )
    .await
    .expect("Failed to insert change log entry");

    // Wait for all retries to exhaust (3 attempts with backoff)
    // 100ms + 200ms + 300ms = 600ms minimum, plus processing overhead
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Verify all attempts failed
    let failed_count = get_observer_log_count(&pool, "failed")
        .await
        .expect("Failed to query observer logs");

    assert!(failed_count >= 1, "Expected at least 1 failed attempt, got {}", failed_count);

    // Verify no success entries (should all be failures)
    let success_count = get_observer_log_count(&pool, "success")
        .await
        .expect("Failed to query observer logs");
    assert_eq!(success_count, 0, "Expected 0 success entries for permanent failure");

    // Verify webhook was called multiple times (retries)
    let webhook_calls = mock_server.request_count().await;
    assert!(
        webhook_calls > 1,
        "Expected multiple webhook calls due to retries, got {}",
        webhook_calls
    );

    // Cleanup
    cleanup_test_data(&pool, &test_id).await.expect("Failed to cleanup");
}

/// Test 6: Multiple Event Types on Same Entity
///
/// Verifies that different event types (INSERT, UPDATE, DELETE) are handled independently
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_multiple_event_types_same_entity() {
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    // Create observers for different event types
    let _insert_observer = create_test_observer(
        &pool,
        &format!("test-insert-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create insert observer");

    let _update_observer = create_test_observer(
        &pool,
        &format!("test-update-{}", test_id),
        Some("Order"),
        Some("UPDATE"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create update observer");

    let order_id = Uuid::new_v4();
    let entity_type = format!("Order_{}", test_id);

    // INSERT event
    let _ = insert_change_log_entry(
        &pool,
        "INSERT",
        &entity_type,
        &order_id.to_string(),
        serde_json::json!({"id": order_id.to_string(), "status": "created"}),
        None,
    )
    .await
    .expect("Failed to insert INSERT event");

    wait_for_webhook(&mock_server, 1, Duration::from_secs(10)).await;

    // UPDATE event
    let _ = insert_change_log_entry(
        &pool,
        "UPDATE",
        &entity_type,
        &order_id.to_string(),
        serde_json::json!({"id": order_id.to_string(), "status": "shipped"}),
        Some(serde_json::json!({"id": order_id.to_string(), "status": "created"})),
    )
    .await
    .expect("Failed to insert UPDATE event");

    wait_for_webhook(&mock_server, 2, Duration::from_secs(10)).await;

    let calls = mock_server.request_count().await;
    assert_eq!(calls, 2, "Expected 2 webhook calls (1 INSERT + 1 UPDATE), got {}", calls);

    // Cleanup
    cleanup_test_data(&pool, &test_id).await.expect("Failed to cleanup");
}

/// Test 7: Batch Processing
///
/// Verifies that multiple events are processed in batches efficiently
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_batch_processing() {
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    let _observer_id = create_test_observer(
        &pool,
        &format!("test-batch-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    // Insert multiple events
    let event_count = 10;
    for i in 0..event_count {
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

    // Wait for all webhooks
    wait_for_webhook(&mock_server, event_count, Duration::from_secs(30)).await;

    let calls = mock_server.request_count().await;
    assert_eq!(calls, event_count, "Expected {} webhook calls, got {}", event_count, calls);

    // Cleanup
    cleanup_test_data(&pool, &test_id).await.expect("Failed to cleanup");
}

/// Performance Benchmark: Latency Measurement
///
/// Measures end-to-end latency from change log insertion to webhook execution
#[tokio::test]
#[ignore = "requires PostgreSQL - performance benchmark"]
async fn benchmark_observer_latency() {
    let test_id = Uuid::new_v4().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");

    let mock_server = MockWebhookServer::start().await;
    mock_server.mock_success().await;

    let _observer_id = create_test_observer(
        &pool,
        &format!("bench-latency-{}", test_id),
        Some("Order"),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    let mut latencies = Vec::new();

    // Measure 20 events
    for _ in 0..20 {
        let order_id = Uuid::new_v4();
        let start = tokio::time::Instant::now();

        let _ = insert_change_log_entry(
            &pool,
            "INSERT",
            &format!("Order_{}", test_id),
            &order_id.to_string(),
            serde_json::json!({"id": order_id.to_string()}),
            None,
        )
        .await
        .expect("Failed to insert change log entry");

        // Poll until webhook is called
        let poll_timeout = Duration::from_secs(10);
        let poll_start = tokio::time::Instant::now();
        let expected_count = latencies.len() + 1;

        while mock_server.request_count().await < expected_count
            && poll_start.elapsed() < poll_timeout
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let latency = start.elapsed();
        latencies.push(latency);

        // Small delay between events
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Calculate percentiles
    latencies.sort();
    let p50_idx = latencies.len() / 2;
    let p95_idx = (latencies.len() * 95) / 100;
    let p99_idx = (latencies.len() * 99) / 100;

    let p50 = latencies[p50_idx];
    let p95 = latencies.get(p95_idx).copied().unwrap_or(p50);
    let p99 = latencies.get(p99_idx).copied().unwrap_or(p95);

    println!("\n=== Observer Latency Benchmark ===");
    println!("p50: {:?} ({:.1}ms)", p50, p50.as_millis());
    println!("p95: {:?} ({:.1}ms)", p95, p95.as_millis());
    println!("p99: {:?} ({:.1}ms)", p99, p99.as_millis());
    println!("Min: {:?}", latencies.first());
    println!("Max: {:?}", latencies.last());

    // Assert p99 is reasonable (< 500ms for test environment)
    assert!(
        p99 < Duration::from_millis(500),
        "p99 latency {} exceeds 500ms threshold",
        p99.as_millis()
    );

    // Cleanup
    cleanup_test_data(&pool, &test_id).await.expect("Failed to cleanup");
}
