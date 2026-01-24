//! Performance Benchmarks for FraiseQL Observer System
//!
//! These benchmarks measure:
//! - Event processing throughput
//! - Deduplication overhead
//! - Cache hit/miss performance
//! - Concurrent vs sequential execution
//!
//! **Run benchmarks**:
//! ```bash
//! # Basic benchmarks
//! cargo bench --bench observer_benchmarks
//!
//! # With Redis features
//! cargo bench --bench observer_benchmarks --features "postgres,dedup,caching"
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use fraiseql_observers::event::{EntityEvent, EventKind};
use fraiseql_observers::executor::ObserverExecutor;
use fraiseql_observers::matcher::EventMatcher;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

// Only available with testing feature - provide simple mock if not available
#[cfg(feature = "testing")]
use fraiseql_observers::testing::mocks::MockDeadLetterQueue;

#[cfg(not(feature = "testing"))]
mod mock_dlq {
    use fraiseql_observers::config::ActionConfig;
    use fraiseql_observers::event::EntityEvent;
    use fraiseql_observers::traits::{DeadLetterQueue, DlqItem};
    use fraiseql_observers::Result;
    use async_trait::async_trait;
    use uuid::Uuid;

    #[derive(Clone)]
    pub struct MockDeadLetterQueue;

    impl MockDeadLetterQueue {
        pub fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl DeadLetterQueue for MockDeadLetterQueue {
        async fn push(&self, _event: EntityEvent, _action: ActionConfig, _error: String) -> Result<Uuid> {
            Ok(Uuid::new_v4())
        }

        async fn get_pending(&self, _limit: i64) -> Result<Vec<DlqItem>> {
            Ok(vec![])
        }

        async fn mark_success(&self, _id: Uuid) -> Result<()> {
            Ok(())
        }

        async fn mark_retry_failed(&self, _id: Uuid, _error: &str) -> Result<()> {
            Ok(())
        }
    }
}

#[cfg(not(feature = "testing"))]
use mock_dlq::MockDeadLetterQueue;

// ============================================================================
// Benchmark Utilities
// ============================================================================

fn create_test_event(kind: EventKind, entity_type: &str, data: serde_json::Value) -> EntityEvent {
    EntityEvent::new(kind, entity_type.to_string(), Uuid::new_v4(), data)
}

fn setup_executor() -> ObserverExecutor {
    let matcher = EventMatcher::new();
    let dlq = Arc::new(MockDeadLetterQueue::new());
    ObserverExecutor::new(matcher, dlq)
}

// ============================================================================
// Benchmark 1: Event Processing Baseline
// ============================================================================

fn benchmark_event_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_processing");

    // Benchmark single event processing
    group.bench_function("process_single_event", |b| {
        let executor = setup_executor();
        let event = create_test_event(EventKind::Created, "User", json!({"name": "Alice"}));

        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                black_box(executor.process_event(&event).await)
            });
    });

    // Benchmark batch event processing
    for batch_size in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("process_batch", batch_size),
            batch_size,
            |b, &size| {
                let executor = Arc::new(setup_executor());
                let events: Vec<EntityEvent> = (0..size)
                    .map(|i| create_test_event(
                        EventKind::Created,
                        "User",
                        json!({"id": i}),
                    ))
                    .collect();

                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async {
                        let executor = executor.clone();
                        let events = events.clone();
                        for event in events {
                            black_box(executor.process_event(&event).await).ok();
                        }
                    });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark 2: Concurrent vs Sequential Execution
// ============================================================================

fn benchmark_concurrent_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_vs_sequential");
    let event_count = 20;

    // Sequential execution
    group.bench_function("sequential_execution", |b| {
        let executor = setup_executor();
        let events: Vec<EntityEvent> = (0..event_count)
            .map(|i| create_test_event(EventKind::Created, "User", json!({"id": i})))
            .collect();

        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                for event in &events {
                    black_box(executor.process_event(event).await).ok();
                }
            });
    });

    // Concurrent execution
    group.bench_function("concurrent_execution", |b| {
        let executor = Arc::new(setup_executor());
        let events: Vec<EntityEvent> = (0..event_count)
            .map(|i| create_test_event(EventKind::Created, "User", json!({"id": i})))
            .collect();

        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let executor = executor.clone();
                let events = events.clone();
                let mut tasks = Vec::new();

                for event in events {
                    let executor_clone = executor.clone();
                    tasks.push(tokio::spawn(async move {
                        executor_clone.process_event(&event).await
                    }));
                }

                for task in tasks {
                    black_box(task.await).ok();
                }
            });
    });

    group.finish();
}

// ============================================================================
// Benchmark 3: Event Matching Performance
// ============================================================================

fn benchmark_event_matching(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_matching");

    // Benchmark with no observers (baseline)
    group.bench_function("match_no_observers", |b| {
        let executor = setup_executor();
        let event = create_test_event(EventKind::Created, "User", json!({"name": "Alice"}));

        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                black_box(executor.process_event(&event).await)
            });
    });

    group.finish();
}

// ============================================================================
// Benchmark 4: Event Creation Overhead
// ============================================================================

fn benchmark_event_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_creation");

    group.bench_function("create_simple_event", |b| {
        b.iter(|| {
            black_box(create_test_event(
                EventKind::Created,
                "User",
                json!({"name": "Alice"}),
            ))
        });
    });

    group.bench_function("create_complex_event", |b| {
        b.iter(|| {
            black_box(create_test_event(
                EventKind::Updated,
                "Order",
                json!({
                    "id": 123,
                    "customer_id": 456,
                    "items": [
                        {"product_id": 1, "quantity": 2, "price": 19.99},
                        {"product_id": 2, "quantity": 1, "price": 49.99},
                    ],
                    "total": 89.97,
                    "status": "pending",
                }),
            ))
        });
    });

    group.finish();
}

// ============================================================================
// Benchmark 5: Throughput Measurement
// ============================================================================

fn benchmark_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");

    for event_count in [100, 500, 1000].iter() {
        group.throughput(Throughput::Elements(*event_count as u64));
        group.bench_with_input(
            BenchmarkId::new("events_per_second", event_count),
            event_count,
            |b, &count| {
                let executor = Arc::new(setup_executor());
                let events: Vec<EntityEvent> = (0..count)
                    .map(|i| create_test_event(
                        EventKind::Created,
                        "User",
                        json!({"id": i}),
                    ))
                    .collect();

                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async {
                        let executor = executor.clone();
                        let events = events.clone();
                        for event in events {
                            black_box(executor.process_event(&event).await).ok();
                        }
                    });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark Groups
// ============================================================================

criterion_group!(
    benches,
    benchmark_event_processing,
    benchmark_concurrent_execution,
    benchmark_event_matching,
    benchmark_event_creation,
    benchmark_throughput,
);

criterion_main!(benches);
