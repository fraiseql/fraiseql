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
//!   postgres:18
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
//!
//! **Execution engine:** none
//! **Infrastructure:** PostgreSQL
//! **Parallelism:** safe

#![cfg(feature = "observers")]
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: CLI / test / example / bench code prints to stdout/stderr by design
mod observer_test_helpers;

use std::time::Duration;

use fraiseql_server::observers::runtime::{ObserverRuntime, ObserverRuntimeConfig};
use observer_test_helpers::*;
use uuid::Uuid;

/// Start an observer runtime polling fast enough for tests (#928).
///
/// Before #928 this suite never constructed a runtime at all — nothing polled
/// the change log, so every test waited for webhooks that could not be sent
/// and timed out. Each test now drives a real `ObserverRuntime` and stops it
/// before cleanup.
async fn start_test_runtime(pool: &sqlx::PgPool) -> ObserverRuntime {
    let config = ObserverRuntimeConfig::new(pool.clone()).with_poll_interval(50);
    let mut runtime = ObserverRuntime::new(config);
    runtime.start().await.expect("Failed to start observer runtime");
    runtime
}

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

    // Create observer for this test's unique entity type (#928: it was
    // registered for "Order" while rows carried "Order_{test_id}", so even a
    // running listener could never have matched).
    let entity_type = format!("Order_{}", test_id);
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-observer-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None, // No condition
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    let mut runtime = start_test_runtime(&pool).await;

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
        &entity_type,
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

    // Verify only one success entry for THIS entity (a global count would
    // measure ambient rows from earlier tests, #928)
    let success_count = get_observer_log_count_for_entity(&pool, &order_id.to_string(), "success")
        .await
        .expect("Failed to query observer logs");
    assert_eq!(success_count, 1, "Expected 1 success log entry");

    runtime.stop().await.expect("Failed to stop observer runtime");

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
    let entity_type = format!("Order_{}", test_id);
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-conditional-{}", test_id),
        Some(&entity_type),
        Some("UPDATE"),
        Some("status == 'shipped'"), // DSL condition
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    let mut runtime = start_test_runtime(&pool).await;

    // Case 1: INSERT with status = 'pending' → should NOT fire
    let order_id_1 = Uuid::new_v4();
    let _ = insert_change_log_entry(
        // intentional
        &pool,
        "UPDATE",
        &entity_type,
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
        // intentional
        &pool,
        "UPDATE",
        &entity_type,
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
    assert_eq!(requests[0]["status"], "shipped");

    runtime.stop().await.expect("Failed to stop observer runtime");

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

    // Observer 1: all INSERTs of this test's entity type
    let entity_type = format!("Order_{}", test_id);
    let _observer_id_1 = create_test_observer(
        &pool,
        &format!("test-multi-1-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server_1.webhook_url(),
    )
    .await
    .expect("Failed to create observer 1");

    // Observer 2: same entity type, different webhook
    let _observer_id_2 = create_test_observer(
        &pool,
        &format!("test-multi-2-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server_2.webhook_url(),
    )
    .await
    .expect("Failed to create observer 2");

    let mut runtime = start_test_runtime(&pool).await;

    // Insert single event
    let order_id = Uuid::new_v4();
    let _ = insert_change_log_entry(
        // intentional
        &pool,
        "INSERT",
        &entity_type,
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

    // Verify core.tb_observer_log has 2 success entries for THIS entity
    let success_count = get_observer_log_count_for_entity(&pool, &order_id.to_string(), "success")
        .await
        .expect("Failed to query observer logs");
    assert_eq!(success_count, 2, "Expected 2 success log entries");

    runtime.stop().await.expect("Failed to stop observer runtime");

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

    let entity_type = format!("Order_{}", test_id);
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-retry-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    let mut runtime = start_test_runtime(&pool).await;

    let order_id = Uuid::new_v4();
    let _ = insert_change_log_entry(
        // intentional
        &pool,
        "INSERT",
        &entity_type,
        &order_id.to_string(),
        serde_json::json!({"id": order_id.to_string()}),
        None,
    )
    .await
    .expect("Failed to insert change log entry");

    // Wait for successful webhook (after retries)
    // With exponential backoff: 100ms + 200ms + 300ms = 600ms minimum, plus processing
    wait_for_webhook(&mock_server, 1, Duration::from_secs(15)).await;

    // Verify retry attempts in core.tb_observer_log.
    //
    // Waits for the success row rather than reading once: the webhook landing
    // and the log INSERT are different events, and on a retried dispatch the
    // failed attempts are written first — so a bare read can return a non-empty
    // log whose last row is still `failed`, and the final-status assertion below
    // would fail on timing rather than on behaviour (#999).
    let logs = wait_for_observer_log_status(
        &pool,
        &order_id.to_string(),
        "success",
        Duration::from_secs(10),
    )
    .await;

    // Should have up to 3 attempts in the log
    assert!(!logs.is_empty(), "Expected observer log entries for entity {}", order_id);

    // Verify final status is success
    let final_status = &logs.last().expect("Should have at least one log").0;
    assert_eq!(final_status, "success", "Final status should be success");

    runtime.stop().await.expect("Failed to stop observer runtime");

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

    let entity_type = format!("Order_{}", test_id);
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-dlq-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    let mut runtime = start_test_runtime(&pool).await;

    let order_id = Uuid::new_v4();
    let _ = insert_change_log_entry(
        // intentional
        &pool,
        "INSERT",
        &entity_type,
        &order_id.to_string(),
        serde_json::json!({"id": order_id.to_string()}),
        None,
    )
    .await
    .expect("Failed to insert change log entry");

    // Wait for all retries to exhaust (3 attempts with backoff)
    // 100ms + 200ms + 300ms = 600ms minimum, plus processing overhead
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Verify the failure was logged for THIS entity. The writer's failure status
    // is "failed" — one of the seven `ck_observer_log_status` accepts. It used to
    // be "error", which that CHECK rejects, so on a production-shaped table the
    // INSERT was refused and the row silently vanished (#932). This assertion
    // only means anything because the fixture now carries the constraint.
    let failed_count = get_observer_log_count_for_entity(&pool, &order_id.to_string(), "failed")
        .await
        .expect("Failed to query observer logs");

    assert!(failed_count >= 1, "Expected at least 1 failed log entry, got {}", failed_count);

    // Verify no success entries for this entity (all attempts failed)
    let success_count = get_observer_log_count_for_entity(&pool, &order_id.to_string(), "success")
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

    runtime.stop().await.expect("Failed to stop observer runtime");

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
    let entity_type = format!("Order_{}", test_id);
    let _insert_observer = create_test_observer(
        &pool,
        &format!("test-insert-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create insert observer");

    let _update_observer = create_test_observer(
        &pool,
        &format!("test-update-{}", test_id),
        Some(&entity_type),
        Some("UPDATE"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create update observer");

    let mut runtime = start_test_runtime(&pool).await;

    let order_id = Uuid::new_v4();

    // INSERT event
    let _ = insert_change_log_entry(
        // intentional
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
        // intentional
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

    runtime.stop().await.expect("Failed to stop observer runtime");

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

    let entity_type = format!("Order_{}", test_id);
    let _observer_id = create_test_observer(
        &pool,
        &format!("test-batch-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    let mut runtime = start_test_runtime(&pool).await;

    // Insert multiple events
    let event_count = 10;
    for i in 0..event_count {
        let order_id = Uuid::new_v4();
        let _ = insert_change_log_entry(
            // intentional
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

    // Wait for all webhooks
    wait_for_webhook(&mock_server, event_count, Duration::from_secs(30)).await;

    let calls = mock_server.request_count().await;
    assert_eq!(calls, event_count, "Expected {} webhook calls, got {}", event_count, calls);

    runtime.stop().await.expect("Failed to stop observer runtime");

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

    let entity_type = format!("Order_{}", test_id);
    let _observer_id = create_test_observer(
        &pool,
        &format!("bench-latency-{}", test_id),
        Some(&entity_type),
        Some("INSERT"),
        None,
        &mock_server.webhook_url(),
    )
    .await
    .expect("Failed to create observer");

    let mut runtime = start_test_runtime(&pool).await;

    let mut latencies = Vec::new();

    // Measure 20 events
    for _ in 0..20 {
        let order_id = Uuid::new_v4();
        let start = tokio::time::Instant::now();

        let _ = insert_change_log_entry(
            // intentional
            &pool,
            "INSERT",
            &entity_type,
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

    runtime.stop().await.expect("Failed to stop observer runtime");

    // Cleanup
    cleanup_test_data(&pool, &test_id).await.expect("Failed to cleanup");
}

/// How the change-log row a ticket's webhook is fired from comes to be written.
#[derive(Clone, Copy)]
enum Ticket {
    /// A table of plain columns, written from outside and captured by the shipped trigger.
    CapturedColumns,
    /// A table that keeps the ticket's document in `data`, written from outside and
    /// captured by the shipped trigger, which is told so.
    CapturedDocument,
    /// The same table and trigger, written by a mutation through the executor, whose
    /// outbox stores the entity the function returns.
    Mutated,
    /// A table of plain columns whose type embeds a customer: `review_status` and the
    /// internal `fk_customer`, written from outside and captured as its row.
    CapturedCustomerColumns,
    /// The same table and trigger, written by a mutation whose function returns the
    /// entity as the type's read builds it, the customer embedded.
    MutatedCustomerColumns,
}

/// One ticket whose `review_status` is `in_review`, written as `ticket` says and observed
/// by a webhook whose body is `template`; the runtime is left to deliver it. Returns the
/// bodies the receiver got, each delivery attempt's logged status and error, and the
/// change-log row's `cdc_source` (the capture trigger's mark; a mutation leaves none).
///
/// Nothing in the test writes the change-log document: a producer does.
async fn deliver_ticket(
    ticket: Ticket,
    template: &str,
) -> (Vec<serde_json::Value>, Vec<(String, Option<String>)>, Option<String>) {
    deliver_ticket_when(ticket, template, None).await
}

/// [`deliver_ticket`] to an observer that runs under `condition`.
async fn deliver_ticket_when(
    ticket: Ticket,
    template: &str,
    condition: Option<&str>,
) -> (Vec<serde_json::Value>, Vec<(String, Option<String>)>, Option<String>) {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");
    sqlx::raw_sql(fraiseql_observers::migrations::entity_change_log_contract_sql())
        .execute(&pool)
        .await
        .expect("the change log takes the columns a mutation writes");
    sqlx::raw_sql(fraiseql_observers::migrations::entity_change_log_capture_trigger_sql())
        .execute(&pool)
        .await
        .expect("the capture function installs");

    let entity_type = format!("Ticket_{test_id}");
    let table = format!("public.tb_ticket_{test_id}");
    let (columns, trigger_args) = match ticket {
        Ticket::CapturedColumns => ("review_status TEXT NOT NULL", "'id', ''"),
        Ticket::CapturedCustomerColumns | Ticket::MutatedCustomerColumns => {
            ("review_status TEXT NOT NULL, fk_customer BIGINT NOT NULL", "'id', ''")
        },
        Ticket::CapturedDocument | Ticket::Mutated => {
            ("data JSONB NOT NULL", "'id', '', 'false', 'data'")
        },
    };
    sqlx::raw_sql(&format!(
        "CREATE TABLE {table} (id UUID PRIMARY KEY DEFAULT gen_random_uuid(), {columns});
         CREATE TRIGGER tr_cdc_capture_ins AFTER INSERT ON {table}
             REFERENCING NEW TABLE AS new_table FOR EACH STATEMENT
             EXECUTE FUNCTION core.fn_entity_change_log_capture('{entity_type}', {trigger_args})"
    ))
    .execute(&pool)
    .await
    .expect("the captured table is created");

    let receiver = MockWebhookServer::start().await;
    receiver.mock_success().await;
    sqlx::query(
        "INSERT INTO core.tb_observer
             (name, entity_type, event_type, actions, retry_config, enabled, condition_expression)
         VALUES ($1, $2, 'INSERT', $3, $4, true, $5)",
    )
    .bind(format!("test-template-{test_id}"))
    .bind(&entity_type)
    .bind(serde_json::json!([
        { "type": "webhook", "url": receiver.webhook_url(), "body_template": template }
    ]))
    .bind(serde_json::json!({
        "max_attempts": 1, "backoff": "fixed", "initial_delay_ms": 10, "max_delay_ms": 10
    }))
    .bind(condition)
    .execute(&pool)
    .await
    .expect("the observer is stored");

    let mut runtime = start_test_runtime(&pool).await;
    let ticket_id = match ticket {
        Ticket::CapturedColumns => {
            insert_ticket(&pool, &format!("{table} (review_status) VALUES ('in_review')")).await
        },
        Ticket::CapturedDocument => {
            let row = format!("{table} (data) VALUES ('{{\"review_status\": \"in_review\"}}')");
            insert_ticket(&pool, &row).await
        },
        Ticket::CapturedCustomerColumns => {
            let row = format!("{table} (review_status, fk_customer) VALUES ('in_review', 7)");
            insert_ticket(&pool, &row).await
        },
        Ticket::Mutated | Ticket::MutatedCustomerColumns => {
            create_ticket_by_mutation(&pool, ticket, &entity_type, &table, &test_id).await
        },
    };

    // The log row is written after the dispatch returns, whichever way it went.
    wait_for_observer_logs(&pool, &ticket_id, 1, Duration::from_secs(10)).await;
    runtime.stop().await.expect("Failed to stop observer runtime");
    let attempts: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT status::text, error_message FROM core.tb_observer_log
         WHERE entity_id = $1::uuid ORDER BY attempt_number",
    )
    .bind(&ticket_id)
    .fetch_all(&pool)
    .await
    .expect("the attempts are logged");
    let sources: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT extra_metadata->>'cdc_source' FROM core.tb_entity_change_log
         WHERE object_id = $1::uuid",
    )
    .bind(&ticket_id)
    .fetch_all(&pool)
    .await
    .expect("the change-log rows are read");
    assert_eq!(sources.len(), 1, "one producer wrote the change: {sources:?}");
    let delivered = receiver.received_requests().await;

    sqlx::raw_sql(&format!(
        "DROP FUNCTION IF EXISTS public.fn_create_ticket_{test_id}(text);
         DROP TABLE {table} CASCADE"
    ))
    .execute(&pool)
    .await
    .ok();
    cleanup_test_data(&pool, &test_id).await.expect("Failed to cleanup");
    (delivered, attempts, sources[0].0.clone())
}

/// Write one ticket from outside the engine; `row` is what follows `INSERT INTO`.
async fn insert_ticket(pool: &sqlx::PgPool, row: &str) -> String {
    let (id,): (Uuid,) = sqlx::query_as(&format!("INSERT INTO {row} RETURNING id"))
        .fetch_one(pool)
        .await
        .expect("the row is written");
    id.to_string()
}

/// Write one ticket through a mutation of `entity_type`, executed by the engine: its
/// function stores the document in `table` and returns it as the entity. For a table of
/// plain columns it stores the columns and returns the entity with its customer embedded.
async fn create_ticket_by_mutation(
    pool: &sqlx::PgPool,
    ticket: Ticket,
    entity_type: &str,
    table: &str,
    test_id: &str,
) -> String {
    use fraiseql_core::{db::postgres::PostgresAdapter, runtime::Executor};

    let (stored, entity) = match ticket {
        Ticket::MutatedCustomerColumns => (
            "(id, review_status, fk_customer) VALUES (v_id, p_status, 7)".to_string(),
            "jsonb_build_object('id', v_id, 'review_status', p_status, \
             'customer', jsonb_build_object('name', 'Ada'))"
                .to_string(),
        ),
        _ => (
            "(id, data) VALUES (v_id, jsonb_build_object('id', v_id, 'review_status', p_status))"
                .to_string(),
            format!("(SELECT data FROM {table} WHERE id = v_id)"),
        ),
    };
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA IF NOT EXISTS app;
         DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',
             'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',
             'rate_limited','service_unavailable');
         EXCEPTION WHEN duplicate_object THEN NULL; END $$;
         DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN,
             state_changed BOOLEAN, error_class app.mutation_error_class, status_detail TEXT,
             http_status SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB,
             updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB);
         EXCEPTION WHEN duplicate_object THEN NULL; END $$;
         CREATE FUNCTION public.fn_create_ticket_{test_id}(p_status text)
         RETURNS app.mutation_response LANGUAGE plpgsql AS $$
         DECLARE v_id uuid := gen_random_uuid(); v_response app.mutation_response;
         BEGIN
             INSERT INTO {table} {stored};
             v_response.succeeded := true; v_response.state_changed := true;
             v_response.entity_type := '{entity_type}'; v_response.entity_id := v_id;
             v_response.entity := {entity};
             RETURN v_response;
         END; $$"
    ))
    .execute(pool)
    .await
    .expect("the mutation function is created");

    let schema = serde_json::from_value(serde_json::json!({
        "naming_convention": "camelCase",
        "types": [{
            "name": entity_type,
            "sql_source": table,
            "fields": [
                { "name": "id", "field_type": "ID" },
                { "name": "reviewStatus", "field_type": "String" }
            ]
        }],
        "mutations": [{
            "name": "createTicket",
            "return_type": entity_type,
            "sql_source": format!("public.fn_create_ticket_{test_id}"),
            "operation": { "Insert": { "table": table } },
            "arguments": [{ "name": "status", "arg_type": "String", "nullable": false }]
        }]
    }))
    .expect("the schema parses");
    let adapter = PostgresAdapter::new(&fraiseql_test_support::database_url())
        .await
        .expect("the adapter");
    let executor = Executor::new(schema, std::sync::Arc::new(adapter));
    let answer = executor
        .execute("mutation { createTicket(status: \"in_review\") { id reviewStatus } }", None)
        .await
        .expect("the mutation executes");
    assert_eq!(
        answer.pointer("/data/createTicket/reviewStatus"),
        Some(&serde_json::json!("in_review")),
        "{answer}"
    );
    answer
        .pointer("/data/createTicket/id")
        .and_then(serde_json::Value::as_str)
        .expect("the mutation returns the ticket's id")
        .to_string()
}

/// #1999: a placeholder is filled from the key the row holds. A column `review_status`
/// reaches the template as `review_status`, which is the name the admin API's template
/// check accepts for a field published as `reviewStatus`.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_webhook_template_is_filled_from_the_key_the_captured_row_holds() {
    let (delivered, attempts, _) =
        deliver_ticket(Ticket::CapturedColumns, r#"{"status": "{{ review_status }}"}"#).await;

    assert_eq!(delivered, vec![serde_json::json!({ "status": "in_review" })], "{attempts:?}");
}

/// The published name is not a key of the row: the event is not delivered. This is the
/// template the check refuses.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_webhook_template_naming_the_published_field_is_not_delivered() {
    let (delivered, attempts, _) =
        deliver_ticket(Ticket::CapturedColumns, r#"{"status": "{{ reviewStatus }}"}"#).await;

    assert!(delivered.is_empty(), "nothing reaches the receiver: {delivered:?}");
    assert_eq!(attempts.len(), 1, "one attempt: {attempts:?}");
    let (status, error) = &attempts[0];
    let error = error.as_deref().unwrap_or_default();
    assert_eq!(status, "failed");
    assert!(error.contains("`{{ reviewStatus }}`"), "names the placeholder: {error}");
}

/// #2013: a table that keeps its entity document in a column is written by a mutation and
/// from outside, and both changes are stored as that document. One template is filled
/// from either, which no template was while the captured change held the table's row.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_webhook_template_is_filled_whichever_producer_wrote_the_change() {
    let template = r#"{"status": "{{ review_status }}"}"#;
    let delivered = vec![serde_json::json!({ "status": "in_review" })];

    let (captured, attempts, source) = deliver_ticket(Ticket::CapturedDocument, template).await;
    assert_eq!(source.as_deref(), Some("fallback_trigger"), "the capture trigger wrote it");
    assert_eq!(captured, delivered, "{attempts:?}");

    let (mutated, attempts, source) = deliver_ticket(Ticket::Mutated, template).await;
    assert_eq!(source, None, "the mutation's outbox wrote it, and the trigger stood down");
    assert_eq!(mutated, delivered, "{attempts:?}");
}

/// The path into the table's document column is a path of neither change: the row is not
/// what is stored. This is the template the admin API's check refuses.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_webhook_template_reaching_into_the_document_column_is_not_delivered() {
    let (delivered, attempts, _) =
        deliver_ticket(Ticket::CapturedDocument, r#"{"status": "{{ data.review_status }}"}"#).await;

    assert!(delivered.is_empty(), "nothing reaches the receiver: {delivered:?}");
    assert_eq!(attempts.len(), 1, "one attempt: {attempts:?}");
    let error = attempts[0].1.as_deref().unwrap_or_default();
    assert_eq!(attempts[0].0, "failed");
    assert!(error.contains("`{{ data.review_status }}`"), "names the placeholder: {error}");
}

/// What one delivery came to: the bodies received, or the error of its one failed attempt.
fn outcome(
    (delivered, attempts, _): (
        Vec<serde_json::Value>,
        Vec<(String, Option<String>)>,
        Option<String>,
    ),
) -> Result<Vec<serde_json::Value>, String> {
    if delivered.is_empty() {
        assert_eq!(attempts.len(), 1, "one attempt: {attempts:?}");
        assert_eq!(attempts[0].0, "failed", "{attempts:?}");
        Err(attempts[0].1.clone().unwrap_or_default())
    } else {
        Ok(delivered)
    }
}

/// #2031: a table of plain columns is written by a mutation and from outside, and the two
/// changes are stored as different images: the entity, its customer embedded, and the
/// table's row. A template reading the embedded customer is filled from the first and not
/// from the second. The compiler and the admin API accept it and warn of the second.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_embedded_field_is_filled_from_a_mutation_and_not_from_a_captured_row() {
    let template = r#"{"customer": "{{ customer.name }}"}"#;

    let mutated = outcome(deliver_ticket(Ticket::MutatedCustomerColumns, template).await);
    assert_eq!(mutated, Ok(vec![serde_json::json!({ "customer": "Ada" })]));

    let captured = outcome(deliver_ticket(Ticket::CapturedCustomerColumns, template).await);
    let error = captured.expect_err("a captured row holds no customer");
    assert!(error.contains("`{{ customer.name }}`"), "names the placeholder: {error}");
}

/// The converse: an internal column is in the captured row and in no entity a mutation
/// returns. This is the template the compiler and the admin API refuse.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_internal_column_is_filled_from_a_captured_row_and_not_from_a_mutation() {
    let template = r#"{"customer": {{ fk_customer }}}"#;

    let captured = outcome(deliver_ticket(Ticket::CapturedCustomerColumns, template).await);
    assert_eq!(captured, Ok(vec![serde_json::json!({ "customer": 7 })]));

    let mutated = outcome(deliver_ticket(Ticket::MutatedCustomerColumns, template).await);
    let error = mutated.expect_err("the entity a mutation returns holds no fk_customer");
    assert!(error.contains("`{{ fk_customer }}`"), "names the placeholder: {error}");
}

// ---------------------------------------------------------------------------
// An observer the compiled schema declares fires with no row seeded by hand
// ---------------------------------------------------------------------------

/// What the receiver got for one change of `entity_type`, once the declared observers are
/// stored as the server stores them at boot and a runtime is started over them.
async fn deliver_declared(
    pool: &sqlx::PgPool,
    declared: &[fraiseql_core::schema::ObserverDefinition],
    entity_type: &str,
    receiver: &MockWebhookServer,
) -> Vec<serde_json::Value> {
    fraiseql_server::observers::ObserverRepository::new(pool.clone())
        .reconcile_declared("schema", declared)
        .await
        .expect("the declared observers are stored");
    receiver.reset().await;
    receiver.mock_success().await;
    let mut runtime = start_test_runtime(pool).await;

    let order_id = Uuid::new_v4();
    insert_change_log_entry(
        pool,
        "INSERT",
        entity_type,
        &order_id.to_string(),
        serde_json::json!({"id": order_id.to_string(), "total": 42}),
        None,
    )
    .await
    .expect("Failed to insert change log entry");
    wait_for_webhook(receiver, 1, Duration::from_secs(10)).await;

    runtime.stop().await.expect("Failed to stop observer runtime");
    receiver.received_requests().await
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_declared_observer_fires_with_no_seeded_row_and_follows_its_declaration() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.expect("Failed to setup schema");
    let receiver = MockWebhookServer::start().await;
    let entity_type = format!("Order_{test_id}");
    let declared_with = |template: &str| {
        vec![
            fraiseql_core::schema::ObserverDefinition::new(
                format!("declared-{test_id}"),
                &entity_type,
                "INSERT",
            )
            .with_action(serde_json::json!({
                "type": "webhook",
                "url": receiver.webhook_url(),
                "body_template": template,
            })),
        ]
    };

    let first = deliver_declared(
        &pool,
        &declared_with(r#"{"release": 1, "total": {{ total }}}"#),
        &entity_type,
        &receiver,
    )
    .await;
    // The declaration is edited and the server redeployed.
    let second = deliver_declared(
        &pool,
        &declared_with(r#"{"release": 2, "total": {{ total }}}"#),
        &entity_type,
        &receiver,
    )
    .await;
    // The declaration is removed and the server redeployed: nothing is left to fire.
    let stored_after_removal: i64 = {
        fraiseql_server::observers::ObserverRepository::new(pool.clone())
            .reconcile_declared("schema", &[])
            .await
            .expect("the removal is stored");
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM core.tb_observer WHERE name = $1 AND deleted_at IS NULL",
        )
        .bind(format!("declared-{test_id}"))
        .fetch_one(&pool)
        .await
        .expect("the stored observers are counted")
    };

    assert_eq!(first, [serde_json::json!({"release": 1, "total": 42})]);
    assert_eq!(second, [serde_json::json!({"release": 2, "total": 42})]);
    assert_eq!(stored_after_removal, 0);

    cleanup_test_data(&pool, &test_id).await.expect("Failed to cleanup");
}

/// How many times an observer under `condition` delivered for `ticket`. The runtime logs a
/// matched observer whether or not its condition held, so the wait ends either way.
async fn deliveries_when(ticket: Ticket, condition: &str) -> usize {
    deliver_ticket_when(ticket, r#"{"ok": true}"#, Some(condition)).await.0.len()
}

/// #2122: a condition reads a top-level key of the event, and the two images of a change
/// to a plain-column table hold different keys. A comparison on the embedded customer
/// holds for a mutation's event and fails to evaluate for a captured row: the compiler
/// and the admin API warn of it.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_condition_on_an_embedded_field_holds_for_a_mutation_and_not_for_a_captured_row() {
    let condition = "customer != null";
    assert_eq!(deliveries_when(Ticket::MutatedCustomerColumns, condition).await, 1);
    assert_eq!(deliveries_when(Ticket::CapturedCustomerColumns, condition).await, 0);
}

/// The converse: a column is in the captured row and in no entity a mutation returns. On
/// a type whose table is captured as its row, the compiler and the admin API warn of it.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_condition_on_a_column_holds_for_a_captured_row_and_not_for_a_mutation() {
    let condition = "fk_customer == 7";
    assert_eq!(deliveries_when(Ticket::CapturedCustomerColumns, condition).await, 1);
    assert_eq!(deliveries_when(Ticket::MutatedCustomerColumns, condition).await, 0);
}

/// A key neither image holds: the observer runs for no event. Where the document is the
/// type's only image, the compiler and the admin API refuse such a condition.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_condition_on_a_key_neither_image_holds_runs_for_no_event() {
    let condition = "fk_custmer == 7";
    assert_eq!(deliveries_when(Ticket::CapturedCustomerColumns, condition).await, 0);
    assert_eq!(deliveries_when(Ticket::MutatedCustomerColumns, condition).await, 0);
}
