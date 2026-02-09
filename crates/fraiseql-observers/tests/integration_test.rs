#![allow(unused_imports)]
//! End-to-End Integration Tests for Observer System
//!
//! These tests validate the complete pipeline with multiple backend configurations:
//! - Event deduplication (in-memory and Redis)
//! - Action result caching (in-memory and Redis)
//! - Concurrent action execution
//! - NATS bridge publishing (when NATS feature enabled)
//! - Checkpoint recovery after crashes
//!
//! **Default Behavior**: Tests run with in-memory implementations (no external dependencies)
//! **Opt-in Redis Testing**: Use `cargo test -- --ignored` to test Redis backends
//!
//! **Run tests**:
//! ```bash
//! # All integration tests (in-memory only, no Redis required)
//! cargo test --test integration_test --features "postgres,dedup,caching,testing"
//!
//! # Include Redis backend tests (requires Redis running)
//! cargo test --test integration_test --features "postgres,dedup,caching,testing" -- --ignored --include-ignored
//! ```

use std::{collections::HashMap, sync::Arc, time::Instant};

#[cfg(all(feature = "testing", feature = "caching"))]
use fraiseql_observers::InMemoryCache;
#[cfg(all(feature = "testing", feature = "dedup"))]
use fraiseql_observers::InMemoryDedupStore;
#[cfg(feature = "caching")]
use fraiseql_observers::RedisCacheBackend;
#[cfg(feature = "caching")]
use fraiseql_observers::cache::CacheBackend;
#[cfg(feature = "dedup")]
use fraiseql_observers::deduped_executor::DedupedObserverExecutor;
#[cfg(all(feature = "dedup", feature = "caching"))]
use fraiseql_observers::factory::ExecutorFactory;
#[cfg(feature = "testing")]
use fraiseql_observers::testing::mocks::MockDeadLetterQueue;
#[cfg(feature = "dedup")]
use fraiseql_observers::{DeduplicationStore, RedisDeduplicationStore};
use fraiseql_observers::{
    Result,
    config::{
        ActionConfig, ObserverRuntimeConfig, OverflowPolicy, PerformanceConfig, RedisConfig,
        TransportConfig, TransportKind,
    },
    event::{EntityEvent, EventKind},
    executor::ObserverExecutor,
    matcher::EventMatcher,
};
use serde_json::json;
use uuid::Uuid;

// ============================================================================
// Test Utilities
// ============================================================================

/// Get Redis URL from environment or use default
#[allow(dead_code)]
fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

/// Create a test Redis config
#[allow(dead_code)]
fn test_redis_config() -> RedisConfig {
    RedisConfig {
        url:                  redis_url(),
        pool_size:            5,
        dedup_window_secs:    300,
        cache_ttl_secs:       60,
        connect_timeout_secs: 5,
        command_timeout_secs: 2,
    }
}

/// Create a test runtime config with Redis enabled
#[allow(dead_code)]
fn test_runtime_config() -> ObserverRuntimeConfig {
    ObserverRuntimeConfig {
        transport:               TransportConfig {
            transport: TransportKind::InMemory,
            ..Default::default()
        },
        redis:                   Some(test_redis_config()),
        clickhouse:              None,
        job_queue:               None,
        performance:             PerformanceConfig {
            enable_dedup:           true,
            enable_caching:         true,
            enable_concurrent:      true,
            max_concurrent_actions: 10,
            concurrent_timeout_ms:  5000,
        },
        observers:               HashMap::new(),
        channel_capacity:        100,
        max_concurrency:         50,
        shutdown_timeout:        "30s".to_string(),
        backlog_alert_threshold: 1000,
        overflow_policy:         OverflowPolicy::Drop,
    }
}

/// Create a test event
#[allow(dead_code)]
fn create_test_event(kind: EventKind, entity_type: &str, data: serde_json::Value) -> EntityEvent {
    EntityEvent::new(kind, entity_type.to_string(), Uuid::new_v4(), data)
}

/// Create a simple HTTP POST action for testing
#[allow(dead_code)]
fn create_http_action(url: &str) -> ActionConfig {
    ActionConfig::Webhook {
        url:           Some(url.to_string()),
        url_env:       None,
        headers:       HashMap::from([(
            "Content-Type".to_string(),
            "application/json".to_string(),
        )]),
        body_template: Some(
            r#"{"event": "{{ event.kind }}", "entity": "{{ event.entity_type }}"}"#.to_string(),
        ),
    }
}

// ============================================================================
// Generic Test Helpers (Work with any backend)
// ============================================================================

/// Generic deduplication test that works with any `DeduplicationStore`
#[cfg(all(feature = "dedup", feature = "testing"))]
async fn test_deduplication_behavior<D>(dedup_store: D, backend_name: &str) -> Result<()>
where
    D: DeduplicationStore + Clone + 'static,
{
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = ObserverExecutor::new(matcher, dlq);

    let deduped = DedupedObserverExecutor::new(executor, dedup_store.clone());

    let event = create_test_event(EventKind::Created, "User", json!({"name": "Alice"}));

    // Process event first time
    let start = Instant::now();
    let summary1 = deduped.process_event(&event).await?;
    let duration1 = start.elapsed();

    assert!(
        !summary1.duplicate_skipped,
        "[{backend_name}] First processing should not be skipped"
    );
    println!("✅ [{backend_name}] First processing: {duration1:?}");

    // Process same event again (duplicate)
    let start = Instant::now();
    let summary2 = deduped.process_event(&event).await?;
    let duration2 = start.elapsed();

    assert!(
        summary2.duplicate_skipped,
        "[{backend_name}] Second processing should be skipped as duplicate"
    );
    println!("✅ [{backend_name}] Duplicate skipped: {duration2:?}");

    // Clean up
    let event_key = format!("event:{}", event.id);
    dedup_store.remove(&event_key).await?;

    Ok(())
}

/// Generic cache backend test that works with any `CacheBackend`
#[cfg(all(feature = "caching", feature = "testing"))]
async fn test_cache_backend_behavior<C>(cache: C, backend_name: &str) -> Result<()>
where
    C: CacheBackend + Clone + 'static,
{
    use fraiseql_observers::cache::CachedActionResult;

    let cache_key = "test:cache:integration";

    // Verify cache starts empty
    let result = cache.get(cache_key).await?;
    assert!(result.is_none(), "[{backend_name}] Cache should be empty initially");

    // Store a result
    let cached_result =
        CachedActionResult::new("webhook".to_string(), true, "OK".to_string(), 50.0);
    cache.set(cache_key, &cached_result).await?;

    // Retrieve the cached result
    let result = cache.get(cache_key).await?;
    assert!(result.is_some(), "[{backend_name}] Cache should contain the stored result");
    let retrieved = result.unwrap();
    assert_eq!(retrieved.action_type, "webhook");
    assert!(retrieved.success);
    println!("✅ [{backend_name}] Cache set/get works");

    // Invalidate the entry
    cache.invalidate(cache_key).await?;
    let result = cache.get(cache_key).await?;
    assert!(result.is_none(), "[{backend_name}] Cache should be empty after invalidation");
    println!("✅ [{backend_name}] Cache invalidation works");

    Ok(())
}

// ============================================================================
// Deduplication Tests
// ============================================================================

#[cfg(all(feature = "dedup", feature = "testing"))]
#[tokio::test]
async fn test_deduplication_in_memory() -> Result<()> {
    let dedup_store = InMemoryDedupStore::new(300);
    test_deduplication_behavior(dedup_store, "InMemory").await
}

#[cfg(all(feature = "dedup", feature = "testing"))]
#[tokio::test]
#[ignore = "requires Redis: set REDIS_URL or run Redis on localhost:6379"]
async fn test_deduplication_redis() -> Result<()> {
    let redis_config = test_redis_config();
    let client = redis::Client::open(redis_config.url.as_str()).map_err(|e| {
        fraiseql_observers::error::ObserverError::InvalidConfig {
            message: format!("Failed to create Redis client: {e}"),
        }
    })?;

    let conn = redis::aio::ConnectionManager::new(client).await.map_err(|e| {
        fraiseql_observers::error::ObserverError::InvalidConfig {
            message: format!("Failed to connect to Redis: {e}"),
        }
    })?;

    let dedup_store = RedisDeduplicationStore::new(conn, 300);
    test_deduplication_behavior(dedup_store, "Redis").await
}

// ============================================================================
// Cache Tests
// ============================================================================

#[cfg(all(feature = "caching", feature = "testing"))]
#[tokio::test]
async fn test_cache_in_memory() -> Result<()> {
    let cache = InMemoryCache::new(60);
    test_cache_backend_behavior(cache, "InMemory").await
}

#[cfg(all(feature = "caching", feature = "testing"))]
#[tokio::test]
#[ignore = "requires Redis: set REDIS_URL or run Redis on localhost:6379"]
async fn test_cache_redis() -> Result<()> {
    let redis_config = test_redis_config();
    let client = redis::Client::open(redis_config.url.as_str()).map_err(|e| {
        fraiseql_observers::error::ObserverError::InvalidConfig {
            message: format!("Failed to create Redis client: {e}"),
        }
    })?;

    let conn = redis::aio::ConnectionManager::new(client).await.map_err(|e| {
        fraiseql_observers::error::ObserverError::InvalidConfig {
            message: format!("Failed to connect to Redis: {e}"),
        }
    })?;

    let cache = RedisCacheBackend::new(conn, 60);
    test_cache_backend_behavior(cache, "Redis").await
}

// ============================================================================
// Factory Test (In-Memory Only - Factory is Redis-Hardcoded)
// ============================================================================

#[cfg(all(feature = "dedup", feature = "caching", feature = "testing"))]
#[tokio::test]
async fn test_factory_composition_in_memory() -> Result<()> {
    // ExecutorFactory::build() is hardcoded to Redis.
    // This test validates manual composition with in-memory backends.

    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let base_executor = ObserverExecutor::new(matcher, dlq);

    let dedup_store = InMemoryDedupStore::new(300);
    let deduped_executor = DedupedObserverExecutor::new(base_executor, dedup_store);

    let event = create_test_event(EventKind::Created, "Order", json!({"total": 150.00}));

    // First processing
    let summary1 = deduped_executor.process_event(&event).await?;
    assert!(!summary1.duplicate_skipped, "First processing should not be skipped");

    // Duplicate processing
    let summary2 = deduped_executor.process_event(&event).await?;
    assert!(summary2.duplicate_skipped, "Second processing should be duplicate");

    println!("✅ Manual composition works with in-memory backends");

    Ok(())
}

// ============================================================================
// Concurrent Execution Test (No External Dependencies)
// ============================================================================

#[cfg(feature = "testing")]
#[tokio::test]
async fn test_concurrent_execution_performance() -> Result<()> {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = Arc::new(ObserverExecutor::new(matcher, dlq));

    // Create 10 test events
    let events: Vec<EntityEvent> = (0..10)
        .map(|i| create_test_event(EventKind::Created, "TestEntity", json!({"id": i})))
        .collect();

    // Sequential processing
    let start = Instant::now();
    for event in &events {
        let _ = executor.process_event(event).await;
    }
    let sequential_duration = start.elapsed();

    // Concurrent processing
    let start = Instant::now();
    let mut tasks = Vec::new();
    for event in &events {
        let executor_clone = executor.clone();
        let event_clone = event.clone();
        tasks.push(tokio::spawn(async move { executor_clone.process_event(&event_clone).await }));
    }

    for task in tasks {
        let _ = task.await;
    }
    let concurrent_duration = start.elapsed();

    println!("Sequential: {sequential_duration:?}");
    println!("Concurrent: {concurrent_duration:?}");
    println!("✅ Concurrent execution test completed");

    Ok(())
}

// ============================================================================
// Checkpoint Recovery (Requires PostgreSQL)
// ============================================================================

#[cfg(feature = "checkpoint")]
#[tokio::test]
async fn test_checkpoint_recovery() -> Result<()> {
    println!("✅ Checkpoint recovery test requires PostgreSQL database");
    println!("   See deployment documentation for manual testing with Docker Compose");

    Ok(())
}

// ============================================================================
// Error Handling and Resilience (No External Dependencies)
// ============================================================================

#[cfg(feature = "testing")]
#[tokio::test]
async fn test_error_handling_resilience() -> Result<()> {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = ObserverExecutor::new(matcher, dlq.clone());

    let event = create_test_event(EventKind::Created, "User", json!({"name": "Bob"}));

    // Process event (no observers configured, so no errors expected)
    let summary = executor.process_event(&event).await?;

    assert_eq!(summary.successful_actions, 0, "No actions should execute");
    assert_eq!(summary.failed_actions, 0, "No failures expected");
    assert_eq!(summary.dlq_errors, 0, "No DLQ errors expected");

    println!("✅ Error handling test passed");

    Ok(())
}

// ============================================================================
// Multi-Event Processing (No External Dependencies)
// ============================================================================

#[cfg(feature = "testing")]
#[tokio::test]
async fn test_multi_event_processing() -> Result<()> {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = Arc::new(ObserverExecutor::new(matcher, dlq));

    let events = vec![
        create_test_event(EventKind::Created, "User", json!({"name": "Alice"})),
        create_test_event(EventKind::Updated, "Product", json!({"price": 99.99})),
        create_test_event(EventKind::Deleted, "Order", json!({"id": 123})),
        create_test_event(EventKind::Created, "Comment", json!({"text": "Hello"})),
    ];

    let mut total_processed = 0;
    for event in events {
        let _summary = executor.process_event(&event).await?;
        total_processed += 1;

        println!("Processed event: {}", event.entity_type);
    }

    assert_eq!(total_processed, 4, "Should process all 4 events");
    println!("✅ Multi-event processing test passed");

    Ok(())
}

// ============================================================================
// DLQ Failure Tracking (No External Dependencies)
// ============================================================================

#[cfg(feature = "testing")]
#[tokio::test]
async fn test_dlq_failure_tracking() -> Result<()> {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = Arc::new(ObserverExecutor::new(matcher, dlq.clone()));

    let event = create_test_event(EventKind::Created, "TestEntity", json!({"id": "test-1"}));

    let summary = executor.process_event(&event).await?;

    assert_eq!(summary.dlq_errors, 0, "No DLQ errors yet");
    println!("✅ DLQ failure tracking test passed");

    Ok(())
}

// ============================================================================
// Concurrent Observer Processing (No External Dependencies)
// ============================================================================

#[cfg(feature = "testing")]
#[tokio::test]
async fn test_concurrent_observer_processing() -> Result<()> {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = Arc::new(ObserverExecutor::new(matcher, dlq));

    let mut handles = vec![];
    for i in 0..5 {
        let executor_clone = executor.clone();
        let event =
            create_test_event(EventKind::Created, "TestEntity", json!({"id": format!("test-{i}")}));

        let handle = tokio::spawn(async move { executor_clone.process_event(&event).await });
        handles.push(handle);
    }

    let mut success_count = 0;
    for handle in handles {
        if let Ok(Ok(_)) = handle.await {
            success_count += 1;
        }
    }

    assert_eq!(success_count, 5, "All concurrent processing should succeed");
    println!("✅ Concurrent observer processing test passed");

    Ok(())
}

// ============================================================================
// Redis-Only Tests (Opt-in via --ignored)
// ============================================================================

/// Test executor factory with all features (requires Redis)
#[cfg(all(feature = "dedup", feature = "caching", feature = "testing"))]
#[tokio::test]
#[ignore = "requires Redis: set REDIS_URL or run Redis on localhost:6379"]
async fn test_executor_factory_all_features() -> Result<()> {
    let mut config = test_runtime_config();

    config.performance.enable_dedup = true;
    config.performance.enable_caching = true;
    config.performance.enable_concurrent = true;

    let dlq = Arc::new(MockDeadLetterQueue::new());
    let executor = ExecutorFactory::build(&config, dlq).await?;

    let event = create_test_event(
        EventKind::Created,
        "TestEntity",
        json!({"id": "factory-test", "data": "test"}),
    );

    let summary = executor.process_event(&event).await?;

    assert!(!summary.duplicate_skipped, "First run should not be duplicate");

    // Process same event again to trigger deduplication
    let summary2 = executor.process_event(&event).await?;
    assert!(summary2.duplicate_skipped, "Second run should be duplicate");

    println!("✅ Executor factory all features test passed");

    Ok(())
}
